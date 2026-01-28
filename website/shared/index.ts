import { PublicKey } from '@solana/web3.js'
import coordinatorIdl from './idl/coordinator_idl.json' with { type: 'json' }
import * as coordinatorTypes from './idl/coordinator_idlType.js'

import miningPoolIdl from './idl/mining-pool_idl.json' with { type: 'json' }
import * as miningPoolTypes from './idl/mining-pool_idlType.js'

type PsycheSolanaCoordinator = coordinatorTypes.PsycheSolanaCoordinator
type PsycheSolanaMiningPool = miningPoolTypes.PsycheSolanaMiningPool

import type {
	GcsRepo,
	HubRepo,
	RunState,
} from 'psyche-deserialize-zerocopy-wasm'

export type * from 'psyche-deserialize-zerocopy-wasm'

export {
	coordinatorIdl,
	coordinatorTypes,
	type PsycheSolanaCoordinator,
	miningPoolIdl,
	miningPoolTypes,
	type PsycheSolanaMiningPool,
}

export { formats, type Version, CURRENT_VERSION } from './formats/index.js'

export interface ChainTimestamp {
	slot: BigInt
	time: Date
}

export interface ContributionInfo {
	totalDepositedCollateralAmount: bigint
	maxDepositCollateralAmount: bigint
	users: Array<{ rank: number; address: string; funding: bigint }>
	collateralMintAddress: string
	// Number of base 10 digits to the right of the decimal place for formatting
	collateralMintDecimals: number
	miningPoolProgramId: string
}

export type ModelType = 'vision' | 'text'

export type RunStatus =
	| { type: 'active' | 'funding' | 'waitingForMembers' | 'paused' }
	| { type: 'completed'; at: ChainTimestamp }

export interface RunSummary {
	id: string

	// there can be an arbitrary number of runs with the same ID as long as you create/destroy them.
	// this is how we track which iteration of a run this is.
	index: number
	isOnlyRunAtThisIndex: boolean

	name: string
	description: string
	status: RunStatus
	pauseHistory: Array<['paused' | 'unpaused', ChainTimestamp]>

	totalTokens: bigint
	lastUpdate: ChainTimestamp
	trainingStep?: {
		lastTokensPerSecond: bigint
		startedAt: ChainTimestamp
		endedAt?: ChainTimestamp
		tokensCompletedAtStartOfStep: bigint
	}

	size: bigint
	type: ModelType
}

export type Metrics = {
	loss: number
	bandwidth: number
	tokensPerSecond: number
	lr: number
	evals: Record<string, number>
	promptResults: number[]
	promptIndex: number
	cumulativePromptResults: number[]
}

export type OverTime<T extends object> = {
	[K in keyof T]: T[K] extends object
		? OverTime<T[K]>
		: Array<readonly [number, T[K]]>
}

export type NullableRecursive<T extends object> = {
	[K in keyof T]: T[K] extends object ? NullableRecursive<T[K]> : T[K] | null
}

export interface RunRoundClient {
	pubkey: string
	witness: false | 'waiting' | 'done'
}

export interface TxSummary {
	timestamp: ChainTimestamp
	txHash: string
	pubkey: string
	method: string
	data: string
}

export interface RunData {
	info: RunSummary
	state?: {
		phase: RunState
		phaseStartTime: Date
		epochStartTime: Date
		clients: Array<RunRoundClient>

		checkpoint: { Hub: HubRepo } | { Gcs: GcsRepo } | null

		round: number
		config: {
			epochTime: number
			minClients: number

			warmupTime: number
			cooldownTime: number

			maxRoundTrainTime: number
			roundWitnessTime: number
		}
	}
	recentTxs: Array<TxSummary>
	metrics: {
		summary: NullableRecursive<Metrics>
		history: OverTime<Metrics>
	}
	promptResults: number[]
	promptIndex: number
	cumulativePromptResults: number[]
}

interface ChainStatus {
	chainSlotHeight: number
	indexedSlot: number
	programId: string
	networkGenesis: string
}

export interface IndexerStatus {
	initTime: number
	commit: string
	coordinator: CoordinatorStatus
	miningPool: MiningPoolStatus
}

export interface CoordinatorStatus {
	status: 'ok' | string
	errors: Array<{ time: Date; error: unknown }>
	chain: ChainStatus
	trackedRuns: Array<{ id: string; index: number; status: RunStatus }>
}

export interface MiningPoolStatus {
	status: 'ok' | string
	errors: Array<{ time: Date; error: unknown }>
	chain: ChainStatus
}

export type MaybeError<T extends object> = T & {
	error?: Error | null | undefined
}

export type ApiGetRun = MaybeError<{ run: RunData | null; isOnlyRun: boolean }>
export type ApiGetRuns = MaybeError<{
	runs: RunSummary[]
	totalTokens: bigint
	totalTokensPerSecondActive: bigint
}>
export type ApiGetCheckpointStatus = MaybeError<{
	isValid: boolean
}>
export type ApiGetContributionInfo = MaybeError<ContributionInfo>

export function u64ToLeBytes(value: bigint) {
	const buffer = new ArrayBuffer(8)
	const view = new DataView(buffer)
	view.setBigUint64(0, value, true)
	return new Uint8Array(buffer)
}

export function getRunPDA(coordinatorProgramId: PublicKey, runId: string) {
	const e = new TextEncoder()
	return PublicKey.findProgramAddressSync(
		[e.encode('coordinator'), e.encode(runId)],
		coordinatorProgramId
	)[0]
}

const poolSeedPrefix = new Uint8Array(
	miningPoolIdl.instructions
		.find((acc) => acc.name === 'pool_create')!
		.accounts.find((acc) => acc.name === 'pool')!.pda!.seeds[0].value!
)
export function getMiningPoolPDA(
	miningPoolProgramId: PublicKey,
	index: bigint
) {
	return PublicKey.findProgramAddressSync(
		[poolSeedPrefix, u64ToLeBytes(index)],
		miningPoolProgramId
	)[0]
}

const lenderSeedPrefix = new Uint8Array(
	miningPoolIdl.instructions
		.find((acc) => acc.name === 'lender_create')!
		.accounts.find((acc) => acc.name === 'lender')!.pda!.seeds[0].value!
)
export function findLender(
	miningPoolProgramId: PublicKey,
	psychePoolPda: PublicKey,
	publicKey: PublicKey
) {
	return PublicKey.findProgramAddressSync(
		[lenderSeedPrefix, psychePoolPda.toBytes(), publicKey.toBytes()],
		miningPoolProgramId
	)[0]
}
