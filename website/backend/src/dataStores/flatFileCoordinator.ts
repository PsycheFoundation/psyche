import path from 'path'
import {
	CoordinatorConfig,
	Model,
	PsycheCoordinator,
	RunMetadata,
	lr_at_step,
} from 'psyche-deserialize-zerocopy-wasm'
import {
	RunSummary,
	RunData,
	Metrics,
	OverTime,
	ChainTimestamp,
	getRunPDA,
	RunRoundClient,
	TxSummary,
	Version,
} from 'shared'
import { CoordinatorDataStore, LastUpdateInfo } from '../dataStore.js'
import { WitnessMetadata, WitnessEvalResult } from '../idlTypes.js'
import { PublicKey } from '@solana/web3.js'
import { isClientWitness } from '../witness.js'
import EventEmitter from 'events'
import { UniqueRunKey, runKey } from '../coordinator.js'
import { readVersionedFile, writeVersionedFile } from './versioned.js'
import { CURRENT_VERSION, CurrentVersion } from 'shared/formats/type.js'
import { existsSync, renameSync } from 'fs'

// any run ID outside this list will not be returned to the frontend in the summary list,
const ALLOWLISTED_RUN_IDS =
	process.env.NODE_ENV === 'development'
		? null
		: ['consilience-40b-1', 'hermes-3-8b', 'hermes-3-8b-2', 'hermes-4-8b']

type Witness = Omit<WitnessMetadata, 'evals'> & {
	evals: Array<[string, number]>
}

interface RunHistory {
	runId: string
	createdAt: ChainTimestamp
	destroyedAt: ChainTimestamp | null
	lastUpdated: ChainTimestamp

	lastState: PsycheCoordinator | null

	configChanges: Array<{
		timestamp: ChainTimestamp
		model: Model
		config: CoordinatorConfig
		metadata: RunMetadata
	}>

	trainingStep?: {
		startedAt: ChainTimestamp
		endedAt?: ChainTimestamp
		tokensCompletedAtStartOfStep: bigint
	}

	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>

	lastFewWitnessUpdates: Array<[Witness, ChainTimestamp]>
	sampledWitnessUpdates: Array<[Witness, ChainTimestamp]>
	sampledWitnessStep?: number

	observedLrByStep: Array<[number, number]>

	recentTxs: Array<TxSummary>
}

interface RunSummaries {
	runs: RunSummary[]
	totalTokens: bigint
	totalTokensPerSecondActive: bigint
}

export class FlatFileCoordinatorDataStore implements CoordinatorDataStore {
	#runs: Map<string, RunHistory[]> = new Map()
	#lastUpdateInfo: LastUpdateInfo = {
		time: new Date(),
		highestSignature: undefined,
	}
	#db: string
	#programId: PublicKey

	#runsMutatedSinceLastSync: Set<UniqueRunKey> = new Set()
	eventEmitter: EventEmitter<{ update: [runKey: UniqueRunKey] }> =
		new EventEmitter()

	// try to mitigate the compute cost of requests by caching runs we've looked up
	#summaryCache: RunSummaries | null = null
	#runCache: Map<UniqueRunKey, RunData> = new Map()

	constructor(dir: string, programId: PublicKey) {
		this.#db = path.join(dir, `./coordinator-db-${programId}.json`)
		this.#programId = programId
		console.log(`loading coordinator db from disk at path ${this.#db}...`)
		try {
			const { version, data } = readVersionedFile(this.#db)
			const { lastUpdateInfo, runs, programId } = tryMigrate(version, data)
			if (this.#programId.equals(programId)) {
				this.#lastUpdateInfo = lastUpdateInfo
				this.#runs = runs
				console.log(
					`loaded DB from disk at slot ${
						this.#lastUpdateInfo.highestSignature?.slot ?? 0
					}`
				)
			} else {
				console.warn(
					`Program ID for coordinator changed from ${programId} in saved state to ${
						this.#programId
					} in args. **Starting from a fresh database**.`
				)
			}
		} catch (err) {
			console.warn('failed to load previous DB from disk: ', err)
			if (existsSync(this.#db)) {
				const randomSuffix = Math.random()
				const badFilename = this.#db + `${randomSuffix}.bak`
				console.warn(`moving existing bad DB file to ${badFilename}`)
				renameSync(this.#db, badFilename)
			}
		}
	}

	#getActiveRun(pubkey: string): [RunHistory, number] {
		const runs = this.#runs.get(pubkey)
		const lastRun = runs?.at(-1)
		if (!runs || !lastRun) {
			throw new Error(
				`Tried to get active run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}

		if (lastRun.destroyedAt) {
			throw new Error(
				`Tried to get active run ${pubkey}, but we saw it shut down at slot ${lastRun.destroyedAt.slot}, and we haven't seen a create since.`
			)
		}
		return [lastRun, runs.length - 1]
	}

	async sync(lastUpdateInfo: LastUpdateInfo) {
		this.#lastUpdateInfo = lastUpdateInfo

		for (const runKey of this.#runsMutatedSinceLastSync) {
			// clear cache for this run
			this.#runCache.delete(runKey)

			// notify any listeners
			this.eventEmitter.emit('update', runKey)
		}

		// clear summary cache if anything changed
		if (this.#runsMutatedSinceLastSync.size > 0) {
			this.#summaryCache = null
		}

		this.#runsMutatedSinceLastSync.clear()
		await writeVersionedFile(this.#db, {
			lastUpdateInfo: this.#lastUpdateInfo,
			runs: this.#runs,
			programId: this.#programId,
		})
	}

	lastUpdate() {
		return this.#lastUpdateInfo
	}

	createRun(
		pubkey: string,
		runId: string,
		eventTime: ChainTimestamp,
		// it's possible that we never get a state, if the run was created and destroyed while we're offline.
		newState?: PsycheCoordinator
	): void {
		if (!this.#runs.has(pubkey)) {
			this.#runs.set(pubkey, [])
		}
		const runsAtThisAddress = this.#runs.get(pubkey)!
		const lastKnownRun = runsAtThisAddress.at(-1)
		if (lastKnownRun && lastKnownRun.destroyedAt === null) {
			throw new Error(
				`Tried to create run ${pubkey}, but we have existing run at this address, created at slot ${lastKnownRun.createdAt.slot}`
			)
		}
		runsAtThisAddress.push({
			runId,
			createdAt: eventTime,
			destroyedAt: null,
			pauseTimestamps: [],
			lastUpdated: eventTime,
			lastFewWitnessUpdates: [],
			sampledWitnessUpdates: [],
			lastState: newState ?? null,
			observedLrByStep: [],
			configChanges: [],
			recentTxs: [],
		})

		this.#runsMutatedSinceLastSync.add(
			runKey(runId, runsAtThisAddress.length - 1)
		)
	}

	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		eventTime: ChainTimestamp,
		configChanged: boolean
	) {
		const [lastRun, index] = this.#getActiveRun(pubkey)

		// we're entering a training step
		if (
			newState.coordinator.run_state === 'RoundTrain' &&
			(!lastRun.lastState ||
				lastRun.lastState.coordinator.run_state !== 'RoundTrain')
		) {
			const lastState = lastRun.lastState
			const tokensCompletedAtStartOfStep = lastState
				? (() => {
						const c = lastState.coordinator
						const tokensPerSequence = BigInt(c.model.LLM.max_seq_len)
						const batchSizeStart = BigInt(c.config.global_batch_size_start)
						const batchSizeEnd = BigInt(c.config.global_batch_size_end)
						const warmupTokens = c.config.global_batch_size_warmup_tokens
						const currentStep = BigInt(c.progress.step - 1)

						return calculateTokens(
							currentStep,
							tokensPerSequence,
							batchSizeStart,
							batchSizeEnd,
							warmupTokens
						)
					})()
				: 0n

			lastRun.trainingStep = {
				startedAt: eventTime,
				tokensCompletedAtStartOfStep,
			}
		}

		// we're leaving a training step
		if (
			newState.coordinator.run_state !== 'RoundTrain' &&
			lastRun.trainingStep &&
			!lastRun.trainingStep.endedAt
		) {
			lastRun.trainingStep.endedAt = eventTime
		}

		lastRun.lastUpdated = eventTime
		lastRun.lastState = newState

		const step = newState.coordinator.progress.step
		if (step > (lastRun.observedLrByStep.at(-1)?.[0] ?? 0)) {
			const lr = lr_at_step(newState.coordinator.model.LLM.lr_schedule, step)
			lastRun.observedLrByStep.push([step, lr])
		}

		if (configChanged) {
			lastRun.configChanges.push({
				timestamp: eventTime,
				config: newState.coordinator.config,
				model: newState.coordinator.model,
				metadata: newState.metadata,
			})
		}

		this.#runsMutatedSinceLastSync.add(runKey(lastRun.runId, index))
	}

	setRunPaused(pubkey: string, paused: boolean, timestamp: ChainTimestamp) {
		const [lastRun, index] = this.#getActiveRun(pubkey)
		const newPauseState = paused ? 'paused' : 'unpaused'
		const lastPauseChange = lastRun.pauseTimestamps.at(-1)
		if (lastPauseChange?.[0] === newPauseState) {
			console.warn(
				`[coordinator] WARNING: Setting run ${pubkey} to pause state ${newPauseState} at slot ${timestamp.slot}, but it's already in that state from pause change at slot ${lastPauseChange[1].slot}.`
			)
		}
		lastRun.lastUpdated = timestamp
		lastRun.pauseTimestamps.push([newPauseState, timestamp])

		this.#runsMutatedSinceLastSync.add(runKey(lastRun.runId, index))
	}

	appendRunWitnesses(
		pubkey: string,
		witnesses: [WitnessMetadata, ChainTimestamp][]
	) {
		const runs = this.#runs.get(pubkey)
		const lastRun = runs?.at(-1)
		if (!runs || !lastRun) {
			throw new Error(
				`Tried to get run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}

		for (const [witness, timestamp] of witnesses) {
			// we don't reallllllly care if it's shut down.
			lastRun.lastUpdated = timestamp

			// format evals to nice strings to save tons of space
			const { evals, ...restWitness } = witness

			// could be a bigint, could be a BN, kind of annoying. TODO fix somewhere else.
			const l =
				typeof evals.len === 'object' && evals.len && 'toNumber' in evals.len
					? evals.len.toNumber()
					: Number(evals.len)
			const fixedEvals: Array<[string, number]> = []
			for (const { name, value } of evals.data.slice(
				0,
				l
			) as WitnessEvalResult[]) {
				const firstZero = name[0].findIndex((v) => v === 0)
				const nameStr = Buffer.from(name[0].slice(0, firstZero)).toString(
					'utf-8'
				)
				fixedEvals.push([nameStr, value])
			}

			let witnessUpdate: [Witness, ChainTimestamp] = [
				{ ...restWitness, evals: fixedEvals } as Witness,
				timestamp,
			]
			lastRun.lastFewWitnessUpdates.push(witnessUpdate)
			lastRun.sampledWitnessUpdates.push(witnessUpdate)
		}

		cleanupWitnessUpdates(lastRun)

		this.#runsMutatedSinceLastSync.add(runKey(lastRun.runId, runs.length - 1))
	}

	destroyRun(pubkey: string, timestamp: ChainTimestamp) {
		const runs = this.#runs.get(pubkey)
		const lastRun = runs?.at(-1)
		if (!runs || !lastRun) {
			throw new Error(
				`Tried to get run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}
		if (lastRun.destroyedAt !== null) {
			throw new Error(
				`Tried to destroy run ${pubkey}, but it's already marked as destroyed at slot ${lastRun.destroyedAt.slot} / time ${lastRun.destroyedAt.time}`
			)
		}
		lastRun.lastUpdated = timestamp
		lastRun.destroyedAt = timestamp

		this.#runsMutatedSinceLastSync.add(runKey(lastRun.runId, runs.length - 1))
	}

	trackTx(
		runPubkey: string,
		userPubkey: string,
		method: string,
		data: string,
		txHash: string,
		timestamp: ChainTimestamp
	) {
		const runs = this.#runs.get(runPubkey)
		const lastRun = runs?.at(-1)
		if (!runs || !lastRun) {
			throw new Error(
				`Tried to get run ${runPubkey}, but we have no runs recorded for that pubkey.`
			)
		}
		lastRun.recentTxs.push({
			pubkey: userPubkey,
			data,
			method,
			timestamp,
			txHash,
		})
		const MAX_RECENT_TXS = 25
		if (lastRun.recentTxs.length > MAX_RECENT_TXS) {
			lastRun.recentTxs = lastRun.recentTxs.slice(-MAX_RECENT_TXS)
		}
		this.#runsMutatedSinceLastSync.add(runKey(lastRun.runId, runs.length - 1))
	}

	getRunSummaries(): RunSummaries {
		if (this.#summaryCache) {
			return this.#summaryCache
		}
		const rawRuns = [...this.#runs.values()].flatMap((runs) =>
			runs.map(
				(r, i) =>
					[
						makeRunSummary(
							r,
							i,
							runs.filter((r) => !!r.lastState).length === 1
						),
						r,
					] as const
			)
		)
		const runs = rawRuns
			.map((r) => r[0])
			.filter(
				(r): r is RunSummary =>
					!!r && (!ALLOWLISTED_RUN_IDS || ALLOWLISTED_RUN_IDS.includes(r.id))
			)
		const summaries = {
			runs,
			totalTokens: runs.reduce(
				(sum, run) =>
					sum + (run.trainingStep?.tokensCompletedAtStartOfStep ?? 0n),
				0n
			),
			totalTokensPerSecondActive: runs.reduce((sum, summary) => {
				const ACTIVE_TIMEOUT_MS = 10 * 60 * 1000
				if (
					summary?.status.type !== 'active' ||
					Date.now() - summary.lastUpdate.time.getTime() > ACTIVE_TIMEOUT_MS
				) {
					return sum
				}
				return sum + (summary.trainingStep?.tokensCompletedAtStartOfStep ?? 0n)
			}, 0n),
		}
		this.#summaryCache = summaries
		return summaries
	}

	getNumRuns(): number {
		return [...this.#runs.values()].reduce(
			(sum, runs) =>
				sum +
				runs.filter(
					(r) =>
						r.lastState &&
						(!ALLOWLISTED_RUN_IDS || ALLOWLISTED_RUN_IDS.includes(r.runId))
				).length,
			0
		)
	}

	getRunDataById(runId: string, index: number): RunData | null {
		const cachedRun = this.#runCache.get(runKey(runId, index))
		if (cachedRun) {
			return cachedRun
		}

		const addr = getRunPDA(this.#programId, runId)
		const runsAtThisAddress = this.#runs.get(addr.toString())
		const run = runsAtThisAddress?.at(index ?? -1)
		if (!run) {
			return null
		}
		const realIndex = runsAtThisAddress!.indexOf(run)
		const info = makeRunSummary(
			run,
			realIndex,
			runsAtThisAddress!.filter((r) => !!r.lastState).length === 1
		)
		if (!info) {
			return null
		}

		const sampledWitnessUpdates = run.sampledWitnessUpdates.map(
			(w) => [w[0].step, w[0]] as const
		)

		const evals: Record<
			string,
			Array<readonly [step: number, value: number]>
		> = {}
		for (const [step, r] of sampledWitnessUpdates) {
			for (const [name, value] of r.evals) {
				if (!(name in evals)) {
					evals[name] = []
				}
				evals[name].push([step, value] as const)
			}
		}
		for (const evalName in evals) {
			evals[evalName] = averageSameStepValues(evals[evalName])
		}
		const history: OverTime<Metrics> = {
			bandwidth: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.bandwidth_per_sec] as const)
					.filter(goodNumber)
			),
			loss: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.loss] as const)
					.filter(goodNumber)
			),
			tokensPerSecond: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.tokens_per_sec] as const)
					.filter(goodNumber)
			),
			lr: run.observedLrByStep.filter(goodNumber),
			evals,
		}

		const summary: Metrics = {
			bandwidth: history.bandwidth.at(-1)?.[1] ?? 0,
			loss: history.loss.at(-1)?.[1] ?? Infinity,
			tokensPerSecond: history.tokensPerSecond.at(-1)?.[1] ?? 0,
			lr: run.observedLrByStep.at(-1)?.[1] ?? 0,
			evals: Object.fromEntries(
				Object.entries(evals)
					.map(([k, v]) => [k, v.at(-1)?.[1]] as const)
					.filter((x): x is [string, number] => x[1] !== undefined)
			),
		}

		let state: RunData['state']
		if (run.lastState) {
			const c = run.lastState

			const clients = c.coordinator.epoch_state.clients
			const currentRound =
				c.coordinator.epoch_state.rounds[c.coordinator.epoch_state.rounds_head]
			const witnessStates = clients.map((client, index) => {
				const isWitness = isClientWitness(
					index,
					currentRound.random_seed,
					clients.length,
					c.coordinator.config.witness_nodes
				)
				const witnessStatus = isWitness
					? currentRound.witnesses.some((w) => Number(w.proof.index) === index)
						? 'done'
						: 'waiting'
					: false
				return {
					pubkey: new PublicKey(client.id.signer).toString(),
					witness: witnessStatus,
				} satisfies RunRoundClient
			})

			const checkpoint =
				(typeof c.coordinator.model.LLM.checkpoint === 'object' &&
					(('Hub' in c.coordinator.model.LLM.checkpoint &&
						c.coordinator.model.LLM.checkpoint.Hub) ||
						('P2P' in c.coordinator.model.LLM.checkpoint &&
							c.coordinator.model.LLM.checkpoint.P2P))) ||
				null

			const config = c.coordinator.config
			state = {
				phase: c.coordinator.run_state,
				phaseStartTime: new Date(
					+`${c.coordinator.run_state_start_unix_timestamp.toString()}000`
				),
				round: currentRound.height,

				clients: witnessStates,
				checkpoint,

				config: {
					minClients: config.init_min_clients,
					roundsPerEpoch: config.rounds_per_epoch,
					cooldownTime: Number(config.cooldown_time),
					maxRoundTrainTime: Number(config.max_round_train_time),
					roundWitnessTime: Number(config.round_witness_time),
					warmupTime: Number(config.warmup_time),

					lrSchedule: c.coordinator.model.LLM.lr_schedule,
				},
			}
		}

		const runData = {
			info,
			state,
			recentTxs: run.recentTxs,
			metrics: {
				summary,
				history,
			},
		}
		this.#runCache.set(runKey(runId, index), runData)
		return runData
	}
}

function goodNumber([_, value]: readonly [
	step: number,
	value: number,
]): boolean {
	return Number.isFinite(value) && !Number.isNaN(value)
}

function makeRunSummary(
	run: RunHistory,
	index: number,
	isOnlyRunAtThisIndex: boolean
): RunSummary | null {
	if (!run.lastState) {
		return null
	}
	const c = run.lastState.coordinator

	const tokensPerSequence = BigInt(c.model.LLM.max_seq_len)
	const batchSizeStart = BigInt(c.config.global_batch_size_start)
	const batchSizeEnd = BigInt(c.config.global_batch_size_end)
	const warmupTokens = c.config.global_batch_size_warmup_tokens
	const totalSteps = BigInt(c.config.total_steps)

	const totalTokens = calculateTokens(
		totalSteps,
		tokensPerSequence,
		batchSizeStart,
		batchSizeEnd,
		warmupTokens
	)

	const lastFewWitnesses = run.lastFewWitnessUpdates.slice(-50)
	const lastStep = lastFewWitnesses.at(-1)?.[0].step ?? -1
	const witnessesForLastStep = lastFewWitnesses.filter(
		(w) => w[0].step === lastStep
	)
	const averageTPS = averageSameStepValues(
		witnessesForLastStep.map((w) => [w[0].step, w[0].tokens_per_sec])
	)
	const lastTokensPerSecond = BigInt(Math.floor(averageTPS[0]?.[1] ?? 0))
	const trainingStep: RunSummary['trainingStep'] = run.trainingStep
		? {
				lastTokensPerSecond,
				startedAt: run.trainingStep.startedAt,
				endedAt: run.trainingStep.endedAt,
				tokensCompletedAtStartOfStep:
					run.trainingStep.tokensCompletedAtStartOfStep,
			}
		: undefined

	const summary: RunSummary = {
		arch: c.model.LLM.architecture,
		id: c.run_id,
		index: index,
		isOnlyRunAtThisIndex,
		name: run.lastState.metadata.name,
		description: run.lastState.metadata.description,
		status: run.destroyedAt
			? {
					type: 'completed',
					at: run.destroyedAt,
				}
			: c.run_state === 'Finished'
				? {
						type: 'completed',
						at: run.lastUpdated,
					}
				: run.lastState.coordinator.run_state === 'Paused'
					? {
							type: 'paused',
						}
					: c.run_state === 'WaitingForMembers'
						? { type: 'waitingForMembers' }
						: {
								type: 'active',
							},
		pauseHistory: run.pauseTimestamps,
		totalTokens,
		lastUpdate: run.lastUpdated,
		size: run.lastState.metadata.num_parameters,
		trainingStep,
		type: 'text', // TODO add type / tags? :)
	}
	return summary
}

function cleanupWitnessUpdates(run: RunHistory) {
	console.log(
		'before cleanup witness:',
		run.runId,
		'lastFewWitnessUpdates',
		run.lastFewWitnessUpdates.length,
		'sampledWitnessUpdates',
		run.sampledWitnessUpdates.length,
		'sampledWitnessStep',
		run.sampledWitnessStep
	)

	// Trim witness updates to the last few
	run.lastFewWitnessUpdates = cleanupOverriddenSteps(run.lastFewWitnessUpdates)
	if (run.lastFewWitnessUpdates.length > 200) {
		run.lastFewWitnessUpdates = run.lastFewWitnessUpdates.slice(-100)
	}

	// Sparsify sampled witness updates when needed
	run.sampledWitnessUpdates = cleanupOverriddenSteps(run.sampledWitnessUpdates)
	run.sampledWitnessUpdates = removeUnsampledSteps(
		run.sampledWitnessUpdates,
		run.sampledWitnessStep
	)
	while (run.sampledWitnessUpdates.length > 2000) {
		run.sampledWitnessStep = (run.sampledWitnessStep ?? 1) * 2
		run.sampledWitnessUpdates = removeUnsampledSteps(
			run.sampledWitnessUpdates,
			run.sampledWitnessStep
		)
	}

	console.log(
		'after cleanup witness:',
		run.runId,
		'lastFewWitnessUpdates',
		run.lastFewWitnessUpdates.length,
		'sampledWitnessUpdates',
		run.sampledWitnessUpdates.length,
		'sampledWitnessStep',
		run.sampledWitnessStep
	)
}

function cleanupOverriddenSteps(witnesses: [Witness, ChainTimestamp][]) {
	let newWitnesses = []
	let minValidStep = Infinity
	for (let i = witnesses.length - 1; i >= 0; i--) {
		let witness = witnesses[i]
		let currentStep = witness[0].step
		if (minValidStep >= currentStep) {
			minValidStep = currentStep
			newWitnesses.push(witness)
		}
	}
	return newWitnesses.reverse()
}

function removeUnsampledSteps(
	witnesses: [Witness, ChainTimestamp][],
	sampledStep?: number
) {
	if (!sampledStep || sampledStep <= 1) {
		return witnesses
	}
	let newWitnesses = []
	for (let witness of witnesses) {
		if (witness[0].step % sampledStep === 0) {
			newWitnesses.push(witness)
		}
	}
	return newWitnesses
}

/**
 * The warmup function is actually exponential,
 * since it's based on its own output from the previous step,
 * and transitions to linear after a specific tokens threshold.
 * This is annoying to model, so we just do the recursive calc.
 * */
function calculateTokens(
	step: bigint,
	tokensPerSequence: bigint,
	batchSizeStart: bigint,
	batchSizeEnd: bigint,
	warmupTokens: bigint
): bigint {
	let currentDataIndex = 0n

	for (let i = 0n; i < step; i++) {
		const tokensProcessedBeforeStep = currentDataIndex * tokensPerSequence

		let batchSizeForStep: bigint
		if (tokensProcessedBeforeStep >= warmupTokens) {
			batchSizeForStep = batchSizeEnd
		} else {
			const progress = Number(tokensProcessedBeforeStep) / Number(warmupTokens)
			const batchSize =
				Number(batchSizeStart) +
				(Number(batchSizeEnd) - Number(batchSizeStart)) * progress
			batchSizeForStep = BigInt(Math.round(batchSize))
		}

		currentDataIndex += batchSizeForStep
	}

	return currentDataIndex * tokensPerSequence
}

function averageSameStepValues(
	values: Array<readonly [step: number, value: number]>
): Array<readonly [step: number, value: number]> {
	const groupedByStep = values.reduce<Record<number, number[]>>(
		(acc, [step, value]) => {
			if (!acc[step]) {
				acc[step] = []
			}
			acc[step].push(value)
			return acc
		},
		{}
	)

	return Object.entries(groupedByStep).map(([step, values]) => {
		const mean = values.reduce((sum, val) => sum + val, 0) / values.length
		return [parseInt(step, 10), mean] as const
	})
}

type ValueInMapRecord<MapRecord> =
	MapRecord extends Map<any, infer I> ? I : never

type CurrentFormat = V2

const migrations: Record<
	`${Exclude<Version, CurrentVersion>}`,
	(data: any) => CurrentFormat
> = {
	unversioned: (data: V0) => {
		for (const [_runId, runV0] of data.runs) {
			for (const historyV0 of runV0) {
				for (const witnessV0 of historyV0.witnessUpdates) {
					const evals = witnessV0[0].evals
					for (let i = 0; i < evals.length; i++) {
						evals[i] = [
							evals[i].name,
							evals[i].value,
						] satisfies ValueInMapRecord<
							V1['runs']
						>[number]['witnessUpdates'][number][0]['evals'][number] as any
					}
				}
				let historyV1 = historyV0 as unknown as RunHistoryV1
				let lastFewWitnessUpdates = historyV1.witnessUpdates.slice()
				let sampledWitnessUpdates = historyV1.witnessUpdates.slice()
				historyV1.witnessUpdates = []
				let historyV2 = historyV1 as unknown as RunHistory
				historyV2.lastFewWitnessUpdates = lastFewWitnessUpdates
				historyV2.sampledWitnessUpdates = sampledWitnessUpdates
				historyV2.sampledWitnessStep = undefined
				cleanupWitnessUpdates(historyV2)
			}
		}
		return data as unknown as V2
	},
	1: (data: V1) => {
		for (const [_runId, runV1] of data.runs) {
			for (const historyV1 of runV1) {
				let lastFewWitnessUpdates = historyV1.witnessUpdates.slice()
				let sampledWitnessUpdates = historyV1.witnessUpdates.slice()
				historyV1.witnessUpdates = []
				let historyV2 = historyV1 as unknown as RunHistory
				historyV2.lastFewWitnessUpdates = lastFewWitnessUpdates
				historyV2.sampledWitnessUpdates = sampledWitnessUpdates
				historyV2.sampledWitnessStep = undefined
				cleanupWitnessUpdates(historyV2)
			}
		}
		return data as unknown as V2
	},
}

interface WitnessV0 {
	evals: Array<{
		name: string
		value: number
	}>
}

interface RunHistoryV0 {
	witnessUpdates: Array<[WitnessV0, any]>
}
interface V0 {
	runs: Map<string, RunHistoryV0[]>
}

interface RunHistoryV1 {
	runId: string
	createdAt: ChainTimestamp
	destroyedAt: ChainTimestamp | null
	lastUpdated: ChainTimestamp

	lastState: PsycheCoordinator | null

	configChanges: Array<{
		timestamp: ChainTimestamp
		model: Model
		config: CoordinatorConfig
		metadata: RunMetadata
	}>

	trainingStep?: {
		startedAt: ChainTimestamp
		endedAt?: ChainTimestamp
		tokensCompletedAtStartOfStep: bigint
	}

	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
	witnessUpdates: Array<[Witness, ChainTimestamp]>
	observedLrByStep: Array<[number, number]>

	recentTxs: Array<TxSummary>
}
interface V1 {
	lastUpdateInfo: LastUpdateInfo
	runs: Map<string, RunHistoryV1[]>
	programId: PublicKey
}

interface V2 {
	lastUpdateInfo: LastUpdateInfo
	runs: Map<string, RunHistory[]>
	programId: PublicKey
}

function tryMigrate(version: Version, data: any): CurrentFormat {
	if (version === CURRENT_VERSION) {
		return data
	}
	console.log(`Migrating from ${version} to ${CURRENT_VERSION}!!`)
	return migrations[version](data)
}
