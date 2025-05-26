import { PublicKey } from '@solana/web3.js'
import coordinatorIdl from './idl/coordinator_idl.json' with { type: 'json' }
import * as coordinatorTypes from './idl/coordinator_idlType.js'

import miningPoolIdl from './idl/mining-pool_idl.json' with { type: 'json' }
import * as miningPoolTypes from './idl/mining-pool_idlType.js'

type PsycheSolanaCoordinator = coordinatorTypes.PsycheSolanaCoordinator
type PsycheSolanaMiningPool = miningPoolTypes.PsycheSolanaMiningPool

import type {
	HubRepo,
	LearningRateSchedule,
	LLMArchitecture,
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

	startTime: ChainTimestamp
	pauseHistory: Array<['paused' | 'unpaused', ChainTimestamp]>

	totalTokens: bigint
	completedTokens: bigint

	size: bigint
	arch: LLMArchitecture
	type: ModelType
}

export type Metrics = {
	loss: number
	bandwidth: number
	tokensPerSecond: number
	lr: number
	evals: Record<string, number>
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
		phaseStartTime?: Date
		clients: Array<RunRoundClient>

		checkpoint: HubRepo | null

		round: number
		config: {
			roundsPerEpoch: number
			minClients: number

			warmupTime: number
			cooldownTime: number

			maxRoundTrainTime: number
			roundWitnessTime: number

			lrSchedule: LearningRateSchedule
		}
	}
	recentTxs: Array<TxSummary>
	metrics: {
		summary: NullableRecursive<Metrics>
		history: OverTime<Metrics>
	}
}

interface PublicKeyJSON {
	___type: 'pubkey'
	value: string
}

interface BigIntJSON {
	___type: 'bigint'
	value: string
}

interface DateJSON {
	___type: 'date'
	value: string
}
interface MapJSON {
	___type: 'map'
	value: Array<[string, any]>
}

interface SetJSON {
	___type: 'set'
	value: any[]
}

function isBigIntJSON(obj: any): obj is BigIntJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'bigint' &&
		typeof obj.value === 'string'
	)
}

function isDateJSON(obj: any): obj is DateJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'date' &&
		typeof obj.value === 'string'
	)
}

function isMapJson(obj: any): obj is MapJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'map' &&
		Array.isArray(obj.value)
	)
}

function isSetJson(obj: any): obj is SetJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'set' &&
		Array.isArray(obj.value)
	)
}

function isPublicKeyJson(obj: any): obj is PublicKeyJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'pubkey' &&
		typeof obj.value === 'string'
	)
}

function isBN(obj: any) {
	return (
		obj &&
		typeof obj === 'object' &&
		(('negative' in obj && 'words' in obj && 'length' in obj && 'red' in obj) ||
			'_bn' in obj)
	)
}
function isPublicKey(obj: any) {
	return (
		obj &&
		typeof obj === 'object' &&
		'constructor' in obj &&
		'findProgramAddressSync' in obj.constructor
	)
}

export function psycheJsonReplacer(this: any, key: string): any {
	const value = this[key]
	if (isPublicKey(value)) {
		return {
			___type: 'pubkey',
			value: value.toString(),
		}
	}
	if (typeof value === 'bigint' || isBN(value)) {
		return {
			___type: 'bigint',
			value: value.toString(),
		}
	}
	if (value instanceof Date) {
		return {
			___type: 'date',
			value: value.toString(),
		}
	}
	if (value instanceof Map) {
		return {
			___type: 'map',
			value: [...value.entries()],
		}
	}
	if (value instanceof Set) {
		return {
			___type: 'set',
			value: [...value.values()],
		}
	}
	return value
}

export function psycheJsonReviver(_key: string, value: any): any {
	if (isPublicKeyJson(value)) {
		return new PublicKey(value.value)
	}
	if (isBigIntJSON(value)) {
		return BigInt(value.value)
	}
	if (isDateJSON(value)) {
		return new Date(value.value)
	}
	if (isMapJson(value)) {
		return new Map(value.value)
	}
	if (isSetJson(value)) {
		return new Set(value.value)
	}
	return value
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
