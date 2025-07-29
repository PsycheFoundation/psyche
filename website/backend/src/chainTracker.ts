import { Program } from '@coral-xyz/anchor'
import { Connection } from '@solana/web3.js'
import {
	coordinatorIdl,
	miningPoolIdl,
	PsycheSolanaCoordinator,
	PsycheSolanaMiningPool,
} from 'shared'
import { CoordinatorDataStore, MiningPoolDataStore } from './dataStore.js'
import { startWatchCoordinatorChainLoop } from './coordinatorChainLoop.js'
import { mkdirSync, readdirSync } from 'fs'
import { FlatFileCoordinatorDataStore } from './dataStores/flatFileCoordinator.js'
import { FlatFileMiningPoolDataStore } from './dataStores/flatFileMiningPool.js'
import { startWatchMiningPoolChainLoop } from './miningPoolChainLoop.js'

interface ServiceConfig {
	connection: Connection
	websocketRpcUrl?: string
	addressOverride?: string
	minSlot: number
}

interface TimestampedError {
	time: Date
	error: unknown
}

interface ServiceResult<T> {
	stopped: Promise<void>
	dataStore: T
	errors: TimestampedError[]
}

function discoverExistingCoordinators(stateDirectory: string): string[] {
	try {
		const files = readdirSync(stateDirectory)
		const coordinatorFiles = files.filter(
			(file) => file.startsWith('coordinator-db-') && file.endsWith('.json')
		)

		// Extract program ID from filename: coordinator-db-{programId}.json
		const programIds = coordinatorFiles.map((file) =>
			file.slice('coordinator-db-'.length, -'.json'.length)
		)

		return programIds
	} catch (error) {
		console.warn('Failed to discover existing coordinators:', error)
		return []
	}
}

function createPromiseHandlers<T>(): {
	promise: Promise<T>
	resolve: (value: T) => void
	reject: (reason?: any) => void
} {
	let resolve!: (value: T) => void
	let reject!: (reason?: any) => void
	const promise = new Promise<T>((res, rej) => {
		resolve = res
		reject = rej
	})
	return { promise, resolve, reject }
}

function startCoordinatorService(
	config: ServiceConfig,
	stateDirectory: string,
	cancelled: { cancelled: boolean }
): ServiceResult<CoordinatorDataStore> {
	const { promise: stopped, resolve, reject } = createPromiseHandlers<void>()

	const program = new Program<PsycheSolanaCoordinator>(
		config.addressOverride
			? { ...coordinatorIdl, address: config.addressOverride }
			: (coordinatorIdl as any),
		config
	)

	const dataStore = new FlatFileCoordinatorDataStore(
		stateDirectory,
		program.programId
	)

	const websocketRpcUrl =
		config.websocketRpcUrl ??
		config.connection.rpcEndpoint.replace('http', 'ws')

	const errors: TimestampedError[] = []

	startWatchCoordinatorChainLoop(
		dataStore,
		program,
		websocketRpcUrl,
		config.minSlot,
		cancelled,
		(error) => errors.push({ error, time: new Date() })
	)
		.catch(reject)
		.then(resolve)

	console.log('Coordinator service initialized:')
	console.log(`Coordinator ProgramID: ${program.programId}`)
	console.log(`Coordinator RPC: ${config.connection.rpcEndpoint}`)
	console.log(`Coordinator websocket RPC: ${websocketRpcUrl}`)

	return { stopped, dataStore, errors }
}

function startMiningPoolService(
	config: ServiceConfig,
	stateDirectory: string,
	cancelled: { cancelled: boolean }
): ServiceResult<MiningPoolDataStore> {
	const { promise: stopped, resolve, reject } = createPromiseHandlers<void>()

	const program = new Program<PsycheSolanaMiningPool>(
		config.addressOverride
			? { ...miningPoolIdl, address: config.addressOverride }
			: (miningPoolIdl as any),
		config
	)

	const dataStore = new FlatFileMiningPoolDataStore(
		stateDirectory,
		program.programId
	)

	const websocketRpcUrl =
		config.websocketRpcUrl ??
		config.connection.rpcEndpoint.replace('http', 'ws')

	const errors: TimestampedError[] = []

	startWatchMiningPoolChainLoop(
		dataStore,
		program,
		websocketRpcUrl,
		config.minSlot,
		cancelled,
		(error) => errors.push({ error, time: new Date() })
	)
		.catch(reject)
		.then(resolve)

	console.log('Mining pool service initialized:')
	console.log(`MiningPool ProgramID: ${program.programId}`)
	console.log(`MiningPool RPC: ${config.connection.rpcEndpoint}`)
	console.log(`MiningPool Websocket RPC: ${websocketRpcUrl}`)

	return { stopped, dataStore, errors }
}

export function startIndexingCoordinators(
	solanaConfig: ServiceConfig,
	miningPool: ServiceConfig
): {
	cancel: () => void
	coordinators: Map<string, ServiceResult<CoordinatorDataStore>>
	miningPool: ServiceResult<MiningPoolDataStore>
} {
	const stateDirectory = process.env.STATE_DIRECTORY ?? process.cwd()
	mkdirSync(stateDirectory, { recursive: true })

	const cancelled = { cancelled: false }

	// Start mining pool service
	console.log('Starting mining pool service')
	const miningPoolService = startMiningPoolService(
		miningPool,
		stateDirectory,
		cancelled
	)

	// Discover existing coordinators and add the configured one
	const existingProgramIds = discoverExistingCoordinators(stateDirectory)
	const configuredProgramId =
		solanaConfig.addressOverride || coordinatorIdl.address

	// Create a set of all coordinator program IDs to monitor
	const allProgramIds = new Set([configuredProgramId, ...existingProgramIds])

	console.log('Discovered existing coordinators:', existingProgramIds)
	console.log('All coordinators to monitor:', Array.from(allProgramIds))

	const coordinators = new Map<string, ServiceResult<CoordinatorDataStore>>()

	// Start coordinator services for all program IDs
	for (const programId of allProgramIds) {
		console.log(`Starting coordinator monitoring for: ${programId}`)

		try {
			const coordinatorConfig: ServiceConfig = {
				connection: solanaConfig.connection,
				websocketRpcUrl: solanaConfig.websocketRpcUrl,
				addressOverride: programId,
				minSlot: solanaConfig.minSlot,
			}

			const coordinatorService = startCoordinatorService(
				coordinatorConfig,
				stateDirectory,
				cancelled
			)
			coordinators.set(programId, coordinatorService)
		} catch (error) {
			console.error(
				`Failed to start monitoring for coordinator ${programId}:`,
				error
			)
		}
	}

	return {
		coordinators,
		miningPool: miningPoolService,
		cancel: () => {
			cancelled.cancelled = true
		},
	}
}
