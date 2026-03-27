import { startIndexingCoordinators } from './chainTracker.js'

import Fastify, { FastifyRequest } from 'fastify'
import cors from '@fastify/cors'

import {
	ApiGetContributionInfo,
	ApiGetRun,
	ApiGetRuns,
	formats,
	IndexerStatus,
	miningPoolIdl,
	RunData,
} from 'shared'
import { Connection } from '@solana/web3.js'
import { makeRateLimitedFetch } from './rateLimit.js'
import { PassThrough } from 'node:stream'
import { getRunFromKey, runKey, UniqueRunKey } from './coordinator.js'
import { CURRENT_VERSION } from 'shared/formats/type.js'
import { RunSummariesData } from './dataStore.js'

const requiredEnvVars = ['COORDINATOR_RPC', 'MINING_POOL_RPC'] as const

const replacer = formats[CURRENT_VERSION].replacer

async function main() {
	for (const v of requiredEnvVars) {
		if (!process.env[v]) {
			throw new Error(`env var ${v} is not set.`)
		}
	}

	if (
		process.env.COORDINATOR_MIN_SLOT !== undefined &&
		`${Number.parseInt(process.env.COORDINATOR_MIN_SLOT, 10)}` !==
			process.env.COORDINATOR_MIN_SLOT
	) {
		throw new Error(
			`COORDINATOR_MIN_SLOT is not a valid integer! got ${process.env.COORDINATOR_MIN_SLOT}`
		)
	}

	if (
		process.env.MINING_POOL_MIN_SLOT !== undefined &&
		`${Number.parseInt(process.env.MINING_POOL_MIN_SLOT, 10)}` !==
			process.env.MINING_POOL_MIN_SLOT
	) {
		throw new Error(
			`MINING_POOL_MIN_SLOT is not a valid integer! got ${process.env.MINING_POOL_MIN_SLOT}`
		)
	}

	const coordinatorRpc = new Connection(process.env.COORDINATOR_RPC!, {
		fetch: makeRateLimitedFetch(),
	})

	// if the RPCs are the same, use only one to share rate limits.
	const miningPoolRpc =
		process.env.COORDINATOR_RPC === process.env.MINING_POOL_RPC
			? coordinatorRpc
			: new Connection(process.env.MINING_POOL_RPC!, {
					fetch: makeRateLimitedFetch(),
				})

	const { coordinators, miningPool, cancel } = await startIndexingCoordinators(
		{
			connection: coordinatorRpc,
			addressOverride: process.env.COORDINATOR_PROGRAM_ID,
			websocketRpcUrl: process.env.COORDINATOR_WS_RPC,
			minSlot: Number.parseInt(process.env.COORDINATOR_MIN_SLOT ?? '0'),
		},
		{
			connection: miningPoolRpc,
			addressOverride: process.env.MINING_POOL_PROGRAM_ID,
			websocketRpcUrl: process.env.MINING_POOL_WS_RPC,
			minSlot: Number.parseInt(process.env.MINING_POOL_MIN_SLOT ?? '0'),
		}
	)

	const liveRunListeners: Map<
		UniqueRunKey,
		Set<(runData: RunData) => void>
	> = new Map()

	// Set up event listeners for all coordinators
	for (const [_programId, coordinator] of coordinators) {
		coordinator.dataStore.eventEmitter.addListener('update', (key) => {
			const listeners = liveRunListeners.get(key)
			if (listeners) {
				const [programId, runId, index] = getRunFromKey(key)
				const runData = coordinator.dataStore.getRunDataById(runId, index)
				if (!runData) {
					console.warn(
						`Tried to emit updates for run ${runId} from coordinator ${programId} but it has no data!`
					)
					return
				}
				for (const listener of listeners) {
					try {
						listener(runData)
					} catch (err) {
						console.error(
							`Failed to send run data for run ${runId} from coordinator ${programId} to subscribed client...`
						)
					}
				}
			}
		})
	}

	const liveRunSummaryListeners: Set<(runData: RunSummariesData) => void> =
		new Set()

	// Set up event listeners for run summaries from all coordinators
	for (const [_programId, coordinator] of coordinators) {
		coordinator.dataStore.eventEmitter.addListener('updateSummaries', () => {
			// Aggregate summaries from all coordinators
			let allRuns: any[] = []
			let totalTokens = 0n
			let totalTokensPerSecondActive = 0n

			for (const [_, coord] of coordinators) {
				const coordinatorSummary = coord.dataStore.getRunSummaries()
				allRuns = allRuns.concat(coordinatorSummary.runs)
				totalTokens += coordinatorSummary.totalTokens
				totalTokensPerSecondActive +=
					coordinatorSummary.totalTokensPerSecondActive
			}

			const aggregatedSummaries: RunSummariesData = {
				runs: allRuns,
				totalTokens,
				totalTokensPerSecondActive,
			}

			for (const listener of liveRunSummaryListeners) {
				try {
					listener(aggregatedSummaries)
				} catch (err) {
					console.error(`Failed to send run summaries to subscribed client...`)
				}
			}
		})
	}

	const liveMiningPoolListeners: Set<() => void> = new Set()
	miningPool.dataStore.eventEmitter.addListener('update', () => {
		for (const listener of liveMiningPoolListeners) {
			try {
				listener()
			} catch (err) {
				console.error(
					`Failed to send data for mining pool to subscribed client...`
				)
			}
		}
	})

	const fastify = Fastify({
		logger: true,
	})

	const shutdown = async () => {
		console.log('got shutdown signal, shutting down!')
		cancel()

		try {
			await fastify.close()
		} catch (err) {
			console.error('Error closing fastify:', err)
		}

		const allCoordinatorPromises = Array.from(coordinators.values()).map(
			(c) => c.stopped
		)

		const shutdownTimeout = setTimeout(() => {
			console.error('Shutdown timeout reached, forcing exit!')
			process.exit(1)
		}, 10000)

		try {
			await Promise.all([...allCoordinatorPromises, miningPool.stopped])
			clearTimeout(shutdownTimeout)
			console.log('Clean shutdown completed')
			process.exit(0)
		} catch (err) {
			console.error('Error during shutdown:', err)
			clearTimeout(shutdownTimeout)
			process.exit(1)
		}
	}

	let coordinatorCrashed: Error | null = null
	for (const [programId, coordinator] of coordinators) {
		coordinator.stopped.catch((err) => {
			console.error(`[${Date.now()}] coordinator ${programId} broken: `, err)
			coordinatorCrashed = new Error(err)
		})
	}

	let miningPoolCrashed: Error | null = null
	miningPool.stopped.catch((err) => {
		console.error(`[${Date.now()}] mining pool broken: `, err)
		miningPoolCrashed = new Error(err)
	})

	process.on('SIGTERM', shutdown)

	process.on('SIGINT', shutdown)

	await fastify.register(cors, {
		origin: process.env.CORS_ALLOW_ORIGIN ?? true,
	})

	const initTime = Date.now()

	function getTotalRuns(): number {
		let totalRuns = 0
		for (const [_, coordinator] of coordinators) {
			totalRuns += coordinator.dataStore.getNumRuns()
		}
		return totalRuns
	}

	function getContributionInfo(
		req: FastifyRequest,
		res: Fastify.FastifyReply,
		address?: string
	) {
		const isStreamingRequest = req.headers.accept?.includes(
			'application/x-ndjson'
		)

		const data: ApiGetContributionInfo = {
			...miningPool.dataStore.getContributionInfo(address),
			miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
			error: miningPoolCrashed,
		}

		// set header for streaming/non
		res.header(
			'content-type',
			isStreamingRequest ? 'application/x-ndjson' : 'application/json'
		)

		if (!isStreamingRequest) {
			res.send(JSON.stringify(data, replacer))
			return
		}

		// start streaming newline-delimited json
		const stream = new PassThrough()
		res.send(stream)

		function sendContributionData() {
			const data: ApiGetContributionInfo = {
				...miningPool.dataStore.getContributionInfo(),
				miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
				error: miningPoolCrashed,
			}
			stream.write(JSON.stringify(data, replacer) + '\n')
		}

		// send the initial run data to populate the UI
		sendContributionData()

		// this listener will be called every time we see a state change.
		liveMiningPoolListeners.add(sendContributionData)

		// when the req closes, stop sending them updates
		req.socket.on('close', () => {
			liveMiningPoolListeners.delete(sendContributionData)
			stream.end()
		})
	}
	fastify.get('/contributionInfo', (req: FastifyRequest, res) => {
		getContributionInfo(req, res)
	})
	fastify.get(
		'/contributionInfo/:address',
		(req: FastifyRequest<{ Params: { address?: string } }>, res) => {
			getContributionInfo(req, res, req.params.address)
		}
	)

	fastify.get('/runs', (req, res) => {
		const isStreamingRequest = req.headers.accept?.includes(
			'application/x-ndjson'
		)
		// Aggregate runs from all coordinators
		let allRuns: any[] = []
		let totalTokens = 0n
		let totalTokensPerSecondActive = 0n

		for (const [programId, coordinator] of coordinators) {
			try {
				const coordinatorSummary = coordinator.dataStore.getRunSummaries()
				allRuns = allRuns.concat(coordinatorSummary.runs)
				totalTokens += coordinatorSummary.totalTokens
				totalTokensPerSecondActive +=
					coordinatorSummary.totalTokensPerSecondActive
			} catch (error) {
				console.error(
					`Failed to get run summaries from coordinator ${programId}:`,
					error
				)
			}
		}

		const data: ApiGetRuns = {
			runs: allRuns,
			totalTokens,
			totalTokensPerSecondActive,
			error: coordinatorCrashed,
		}

		// set header for streaming/non
		res.header(
			'content-type',
			isStreamingRequest ? 'application/x-ndjson' : 'application/json'
		)

		if (!isStreamingRequest) {
			res.send(JSON.stringify(data, replacer))
			return
		}

		// start streaming newline-delimited json
		const stream = new PassThrough()
		res.send(stream)

		function sendRunSummariesData(runSummariesData: RunSummariesData) {
			const data: ApiGetRuns = {
				...runSummariesData,
				error: coordinatorCrashed,
			}
			stream.write(JSON.stringify(data, replacer) + '\n')
		}

		// send the initial run summaries data to populate the UI
		sendRunSummariesData(data)

		// this listener will be called every time we see a state change.
		liveRunSummaryListeners.add(sendRunSummariesData)

		// when the req closes, stop sending them updates
		req.socket.on('close', () => {
			liveRunSummaryListeners.delete(sendRunSummariesData)
			stream.end()
		})
	})

	fastify.get(
		'/run/:runId/:programId/:indexStr',
		(
			req: FastifyRequest<{
				Params: { runId?: string; programId?: string; indexStr?: string }
			}>,
			res
		) => {
			const isStreamingRequest = req.headers.accept?.includes(
				'application/x-ndjson'
			)
			const { runId, programId, indexStr } = req.params

			const index = Number.parseInt(indexStr ?? '0')
			if (`${index}` !== indexStr) {
				throw new Error(`Invalid index ${indexStr}`)
			}

			// Find the specific coordinator and run
			let matchingRun: any = null
			let totalRuns = 0

			if (runId && programId) {
				const coordinator = coordinators.get(programId)
				if (coordinator) {
					try {
						matchingRun = coordinator.dataStore.getRunDataById(runId, index)
					} catch (error) {
						console.error(
							`Failed to get run from coordinator ${programId}:`,
							error
						)
					}
				}

				totalRuns = getTotalRuns()
			}

			const data: ApiGetRun = {
				run: matchingRun,
				error: coordinatorCrashed,
				isOnlyRun: totalRuns === 1,
			}

			// set header for streaming/non
			res.header(
				'content-type',
				isStreamingRequest ? 'application/x-ndjson' : 'application/json'
			)

			if (!isStreamingRequest || !matchingRun) {
				res.send(JSON.stringify(data, replacer))
				return
			}

			const key = runKey(
				matchingRun.programId,
				matchingRun.info.id,
				matchingRun.info.index
			)
			let listeners = liveRunListeners.get(key)
			if (!listeners) {
				listeners = new Set()
				liveRunListeners.set(key, listeners)
			}

			// start streaming newline-delimited json
			const stream = new PassThrough()
			res.send(stream)

			function sendRunData(runData: RunData) {
				// Calculate total runs across all coordinators
				const totalRuns = getTotalRuns()

				const data: ApiGetRun = {
					run: runData,
					error: coordinatorCrashed,
					isOnlyRun: totalRuns === 1,
				}
				stream.write(JSON.stringify(data, replacer) + '\n')
			}

			// send the initial run data to populate the UI
			sendRunData(matchingRun)

			// this listener will be called every time we see a state change.
			listeners.add(sendRunData)

			// when the req closes, stop sending them updates
			req.socket.on('close', () => {
				listeners.delete(sendRunData)
				stream.end()
			})
		}
	)

	fastify.get<{
		Querystring: { owner: string; repo: string; revision?: string }
	}>('/check-checkpoint', async (request) => {
		const { owner, repo, revision } = request.query
		const url = `https://huggingface.co/${owner}/${repo}${revision ? `/tree/${revision}` : ''}`
		try {
			const response = await fetch(url, { method: 'HEAD' })
			return { isValid: response.ok, description: response.statusText }
		} catch (error) {
			const errorMessage =
				error instanceof Error ? error.message : 'Unknown error'
			return { isValid: false, description: errorMessage }
		}
	})

	fastify.get<{
		Querystring: { bucket: string; prefix?: string }
	}>('/check-gcs-bucket', async (request) => {
		const { bucket, prefix } = request.query
		const path = prefix ? `${prefix}/` : ''
		const url = `https://storage.googleapis.com/${bucket}/${path}manifest.json`
		try {
			const response = await fetch(url, { method: 'HEAD' })
			return { isValid: response.ok, description: response.statusText }
		} catch (error) {
			const errorMessage =
				error instanceof Error ? error.message : 'Unknown error'
			return { isValid: false, description: errorMessage }
		}
	})

	fastify.get('/status', async (_, res) => {
		// Aggregate status from all coordinators
		const coordinatorStatuses: Record<string, any> = {}
		for (const [programId, coordinator] of coordinators) {
			try {
				coordinatorStatuses[programId] = {
					status: coordinatorCrashed ? coordinatorCrashed.toString() : 'ok',
					errors: coordinator.errors,
					trackedRuns: coordinator.dataStore
						.getRunSummaries()
						.runs.map((r) => ({
							id: r.id,
							index: r.index,
							status: r.status,
							programId: r.programId,
						})),
					chain: {
						chainSlotHeight: await coordinatorRpc.getSlot('confirmed'),
						indexedSlot:
							coordinator.dataStore.lastUpdate().highestSignature?.slot ?? 0,
						programId: programId,
						networkGenesis: await coordinatorRpc.getGenesisHash(),
					},
				}
			} catch (error) {
				coordinatorStatuses[programId] = {
					status: `error: ${error}`,
					errors: [],
					trackedRuns: [],
					chain: { programId },
				}
			}
		}

		const data = {
			commit: process.env.GITCOMMIT ?? '???',
			initTime,
			coordinators: coordinatorStatuses,
			miningPool: {
				status: miningPoolCrashed ? miningPoolCrashed.toString() : 'ok',
				errors: miningPool.errors,
				chain: {
					chainSlotHeight: await miningPoolRpc.getSlot('confirmed'),
					indexedSlot:
						miningPool.dataStore.lastUpdate().highestSignature?.slot ?? 0,
					programId:
						process.env.MINING_POOL_PROGRAM_ID ?? miningPoolIdl.address,
					networkGenesis: await miningPoolRpc.getGenesisHash(),
				},
			},
		} satisfies IndexerStatus
		res
			.header('content-type', 'application/json')
			.send(JSON.stringify(data, replacer))
	})

	await fastify.listen({ host: '0.0.0.0', port: 3000 })
}
main()
