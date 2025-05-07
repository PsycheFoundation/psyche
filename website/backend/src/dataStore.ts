import { PsycheCoordinator } from 'psyche-deserialize-zerocopy-wasm'
import { RunSummary, RunData, ContributionInfo, ChainTimestamp } from 'shared'
import { PsycheMiningPoolAccount, WitnessMetadata } from './idlTypes.js'
import EventEmitter from 'node:events'
import { UniqueRunKey } from './coordinator.js'

export interface IndexedSignature {
	signature: string
	slot: number
}

export interface LastUpdateInfo {
	time: Date
	highestSignature?: IndexedSignature
}

export interface ChainDataStore {
	lastUpdate(): LastUpdateInfo
	sync(lastUpdateInfo: LastUpdateInfo): Promise<void>
	eventEmitter: EventEmitter
}

export interface CoordinatorDataStore extends ChainDataStore {
	eventEmitter: EventEmitter<{ update: [UniqueRunKey] }>

	createRun(
		pubkey: string,
		runId: string,
		timestamp: ChainTimestamp,
		// it's possible that we never get a state, if the run was created then destroyed while we're offline.
		newState?: PsycheCoordinator
	): void
	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		timestamp: ChainTimestamp,
		configChanged: boolean
	): void
	setRunPaused(pubkey: string, paused: boolean, timestamp: ChainTimestamp): void
	witnessRun(
		pubkey: string,
		witness: WitnessMetadata,
		timestamp: ChainTimestamp
	): void
	destroyRun(pubkey: string, timestamp: ChainTimestamp): void

	// called on any tx change
	trackTx(
		runPubkey: string,
		userPubkey: string,
		method: string,
		data: string,
		txHash: string,
		timestamp: ChainTimestamp
	): void

	getRunSummaries(): {
		runs: RunSummary[]
		totalTokens: bigint
		totalTokensPerSecondActive: bigint
	}
	getRunDataById(runId: string, index: number): RunData | null

	getNumRuns(): number
}

export interface MiningPoolDataStore extends ChainDataStore {
	eventEmitter: EventEmitter
	setFundingData(data: PsycheMiningPoolAccount): void
	setUserAmount(address: string, amount: bigint): void
	setCollateralInfo(mintAddress: string, decimals: number): void
	hasCollateralInfo(): boolean
	getContributionInfo(): Omit<ContributionInfo, 'miningPoolProgramId'>
}
