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
import { CURRENT_VERSION } from 'shared/formats/type.js'
import { existsSync, renameSync } from 'fs'

// any run ID outside this list will not be returned to the frontend in the summary list,
const ALLOWLISTED_RUN_IDS =
	process.env.NODE_ENV === 'development'
		? null
		: [
				'consilience-40b-1',
				'hermes-3-8b',
				'hermes-3-8b-2',
				'hermes-4-8b',
				'hermes-4-8b-2',
				'dm-fwedu-baseline',
				'dm-fwedu-baseline-2',
				'dm-dclm-baseline',
				'dm-fwedu-dclm',
				'dm-fwedu-dclm-fpdf',
				'dm-fwedu-dclm-fw2hq',
				'dm-fwedu-dclm-stack',
				'dm-fwedu-dclm-stack-nmath',
				'dm-fwedu-dclm-wiki-pes',
				'dm-consilience-rc1',
				'dm-consilience-rc2',
				'dm-consilience-rc3',
				'dm-consilience-rc4',
				'hermes-4-36b',
				'hermes-4.1-36b',
				'hermes-4.3-36b',
				'hermes-4.3-36b-2',
				'moe-10b-a1b-8k-wsd-lr3e4-1t',
			]

type WitnessV2 = Omit<
	WitnessMetadata,
	'evals' | 'prompt_results' | 'prompt_index'
> & {
	evals: Array<[string, number]>
	prompt_results: number[]
	prompt_index: number
}

interface RunHistoryV2 {
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

	lastFewWitnessUpdates: Array<[WitnessV2, ChainTimestamp]>
	sampledWitnessUpdates: Array<[WitnessV2, ChainTimestamp]>
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
	#runs: Map<string, RunHistoryV2[]> = new Map()
	#lastUpdateInfo: LastUpdateInfo = {
		time: new Date(),
		highestSignature: undefined,
	}
	#db: string
	#programId: PublicKey

	#runsMutatedSinceLastSync: Set<UniqueRunKey> = new Set()
	eventEmitter: EventEmitter<{
		update: [runKey: UniqueRunKey]
		updateSummaries: []
	}> = new EventEmitter()

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

	#getActiveRun(pubkey: string): [RunHistoryV2, number] {
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

		this.eventEmitter.emit('updateSummaries')

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
			if (isGoodNumber(lr)) {
				lastRun.observedLrByStep.push([step, lr])
			}
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
			const { evals, prompt_results, prompt_index, ...restWitness } = witness

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

			// convert FixedVec to regular array
			const promptTokens: number[] = []
			if (prompt_results && prompt_results.data) {
				const promptLen =
					typeof prompt_results.len === 'object' &&
					prompt_results.len &&
					'toNumber' in prompt_results.len
						? prompt_results.len.toNumber()
						: Number(prompt_results.len)
				for (let i = 0; i < promptLen && i < prompt_results.data.length; i++) {
					promptTokens.push(Number(prompt_results.data[i]))
				}
			}

			const witnessUpdate = {
				...restWitness,
				evals: fixedEvals,
				prompt_results: promptTokens,
				prompt_index: prompt_index || 0, // Default to 0 if undefined
			}
			lastRun.lastFewWitnessUpdates.push([witnessUpdate, timestamp])
			lastRun.sampledWitnessUpdates.push([witnessUpdate, timestamp])
		}

		if (witnesses.length > 0) {
			cleanupWitnessUpdates(lastRun)
		}
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
				return sum + (summary.trainingStep?.lastTokensPerSecond ?? 0n)
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

		// collect prompt results by step
		const promptResults: Array<readonly [number, number[]]> = []
		const promptIndices: Array<readonly [number, number]> = []
		const cumulativePromptResults: Array<readonly [number, number[]]> = []

		let cumulativeTokens: number[] = []
		let currentPromptIndex: number | null = null

		for (const [step, r] of sampledWitnessUpdates) {
			// Check if prompt index changed: if so, reset cumulative tokens
			if (r.prompt_index !== undefined && typeof r.prompt_index === 'number') {
				if (
					currentPromptIndex !== null &&
					r.prompt_index !== currentPromptIndex
				) {
					// Prompt changed, reset cumulative tokens
					cumulativeTokens = []
				}
				currentPromptIndex = r.prompt_index
				promptIndices.push([step, r.prompt_index] as const)
			}

			if (
				r.prompt_results &&
				Array.isArray(r.prompt_results) &&
				r.prompt_results.length > 0
			) {
				promptResults.push([step, r.prompt_results] as const)
				// Accumulate tokens for cumulative results (within current prompt)
				cumulativeTokens = [...cumulativeTokens, ...r.prompt_results]
				cumulativePromptResults.push([step, [...cumulativeTokens]] as const)
			}
		}

		const gn = ([_, v]: readonly [number, number]) => isGoodNumber(v)

		const history: OverTime<Metrics> = {
			bandwidth: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.bandwidth_per_sec] as const)
					.filter(gn)
			),
			loss: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.loss] as const)
					.filter(gn)
			),
			tokensPerSecond: averageSameStepValues(
				sampledWitnessUpdates
					.map(([step, h]) => [step, h.tokens_per_sec] as const)
					.filter(gn)
			),
			lr: run.observedLrByStep,
			evals,
			promptResults:
				promptResults as unknown as OverTime<Metrics>['promptResults'],
			promptIndex: promptIndices,
			cumulativePromptResults:
				cumulativePromptResults as unknown as OverTime<Metrics>['cumulativePromptResults'],
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
			promptResults: (history.promptResults.at(-1)?.[1] ?? []) as number[],
			promptIndex: history.promptIndex.at(-1)?.[1] ?? 0,
			cumulativePromptResults: (history.cumulativePromptResults.at(-1)?.[1] ??
				[]) as number[],
		}

		let state: RunData['state']
		if (run.lastState) {
			const c = run.lastState

			const currentRoundClients = c.coordinator.epoch_state.clients
			const currentRound =
				c.coordinator.epoch_state.rounds[c.coordinator.epoch_state.rounds_head]
			const witnessStates = currentRoundClients.map((client, index) => {
				const isWitness = isClientWitness(
					index,
					currentRound.random_seed,
					currentRoundClients.length,
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

			const clients =
				c.coordinator.run_state === 'WaitingForMembers'
					? c.clients_state.clients
							.filter((cl) => cl.active === c.clients_state.next_active)
							.map((cl) => ({
								pubkey: new PublicKey(cl.id.signer).toString(),
								witness: false as const,
							}))
					: witnessStates
			state = {
				phase: c.coordinator.run_state,
				phaseStartTime: new Date(
					+`${c.coordinator.run_state_start_unix_timestamp.toString()}000`
				),
				epochStartTime: new Date(
					+`${c.coordinator.epoch_state.start_timestamp.toString()}000`
				),
				round: currentRound.height,

				clients,
				checkpoint,

				config: {
					minClients: config.init_min_clients,
					epochTime: Number(config.epoch_time),
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
			promptResults: promptResults.at(-1)?.[1] ?? [],
			promptIndex: promptIndices.at(-1)?.[1] ?? 0,
			cumulativePromptResults: cumulativePromptResults.at(-1)?.[1] ?? [],
		}
		this.#runCache.set(runKey(runId, index), runData)
		return runData
	}
}

function isGoodNumber(value: number): boolean {
	return (
		typeof value === 'number' && !Number.isNaN(value) && Number.isFinite(value)
	)
}

function makeRunSummary(
	run: RunHistoryV2,
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

	const lastFewWitnesses = run.lastFewWitnessUpdates
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

function cleanupWitnessUpdates(run: RunHistoryV2) {
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
	run.lastFewWitnessUpdates = cleanupLastFewUpdates(run.lastFewWitnessUpdates)

	// Sparsify sampled witness updates when needed
	const { updates: sampledWitnessUpdates, step: sampledWitnessStep } =
		cleanupSampledUpdates(run.sampledWitnessUpdates, run.sampledWitnessStep)

	run.sampledWitnessStep = sampledWitnessStep
	run.sampledWitnessUpdates = sampledWitnessUpdates

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

function cleanupLastFewUpdates(
	witnesses: [WitnessV2, ChainTimestamp][]
): [WitnessV2, ChainTimestamp][] {
	const withoutOverrides = removeOverriddenSteps(witnesses)
	return withoutOverrides.length > 200
		? withoutOverrides.slice(-100)
		: withoutOverrides
}

/**
 * Remove overridden steps, average values for the same step (except the latest one), then downsample if needed.
 */
function cleanupSampledUpdates(
	witnesses: [WitnessV2, ChainTimestamp][],
	initialStep?: number
): { updates: [WitnessV2, ChainTimestamp][]; step: number } {
	const linearHistory = removeOverriddenSteps(witnesses)

	const latestStep = linearHistory.at(-1)?.[0].step
	const splitIndex =
		linearHistory.findLastIndex(([witness]) => witness.step !== latestStep) + 1

	const latestStepWitnesses = linearHistory.slice(splitIndex)
	const olderWitnesses = linearHistory.slice(0, splitIndex)

	// only aggregate and sample the older witnesses, not the latest step,
	// because we don't want to over-weight new witnesses as they come in for the latest step
	const aggregated = aggregateByStep(olderWitnesses)

	const MAX_SAMPLES = 2000

	const finalSampleStep = calculateOptimalStep(
		aggregated.length,
		MAX_SAMPLES - 2,
		initialStep ?? 1
	)

	const sampled =
		finalSampleStep > 1
			? [
					aggregated[0],
					...aggregated
						.slice(1, -1)
						.filter(([witness]) => witness.step % finalSampleStep === 0),
					aggregated.at(-1)!,
				]
			: aggregated

	// combine the sampled older witnesses with the unprocessed latest step witnesses
	const finalUpdates = [...sampled, ...latestStepWitnesses]

	return { updates: finalUpdates, step: finalSampleStep }
}

/**
 * Returns a single linear history of witnesses.
 * Will sort the input argument in-place.
 */
function removeOverriddenSteps(
	witnesses: [WitnessV2, ChainTimestamp][]
): [WitnessV2, ChainTimestamp][] {
	const validWitnesses = witnesses.filter((w) => {
		if (!w) {
			console.error('null witness found?? removing...')
			return false
		}
		return true
	})
	const orderedWitnesses = validWitnesses.sort(
		(a, b) => a[1].time.getTime() - b[1].time.getTime()
	)

	// Walk backwards, keep only non-overridden steps
	const newWitnesses: [WitnessV2, ChainTimestamp][] = []
	let minValidStep = Infinity

	for (let i = orderedWitnesses.length - 1; i >= 0; i--) {
		const witness = orderedWitnesses[i]
		const currentStep = witness[0].step

		if (minValidStep >= currentStep) {
			minValidStep = currentStep
			newWitnesses.push(witness)
		}
	}

	return newWitnesses.reverse()
}

/**
 * Given a list of witness, averages all witnesses from the same step.
 */
function aggregateByStep(
	witnesses: [WitnessV2, ChainTimestamp][]
): [WitnessV2, ChainTimestamp][] {
	const groups = new Map<number, [WitnessV2, ChainTimestamp][]>()

	for (const witness of witnesses) {
		const step = witness[0].step
		if (!groups.has(step)) {
			groups.set(step, [])
		}
		groups.get(step)!.push(witness)
	}

	// average each group
	const aggregated: [WitnessV2, ChainTimestamp][] = [...groups.values()].map(
		averageWitnessesForStep
	)

	// sort by step
	return aggregated.sort((a, b) => a[0].step - b[0].step)
}

function averageWitnessesForStep(
	witnesses: [WitnessV2, ChainTimestamp][]
): [WitnessV2, ChainTimestamp] {
	const baseWitness = witnesses[0][0]
	const latestTimestamp = witnesses.reduce(
		(latest, [_, timestamp]) =>
			timestamp.time.getTime() > latest.time.getTime() ? timestamp : latest,
		witnesses[0][1]
	)

	const evalGroups = new Map<string, number[]>()
	const propValues = new Map<string, number[]>()

	for (const [witness] of witnesses) {
		// Group evals
		for (const [name, value] of witness.evals) {
			if (!evalGroups.has(name)) {
				evalGroups.set(name, [])
			}
			evalGroups.get(name)!.push(value)
		}

		// Auto-collect all numeric properties (excluding special ones)
		for (const [key, value] of Object.entries(witness)) {
			if (
				key !== 'evals' &&
				key !== 'prompt_results' &&
				key !== 'step' &&
				key !== 'prompt_index' &&
				typeof value === 'number' &&
				isGoodNumber(value)
			) {
				if (!propValues.has(key)) {
					propValues.set(key, [])
				}
				propValues.get(key)!.push(value)
			}
		}
	}

	// average evals
	const averagedEvals: [string, number][] = []
	for (const [name, values] of evalGroups) {
		const mean = values.reduce((sum, val) => sum + val, 0) / values.length
		averagedEvals.push([name, mean])
	}

	const averagedWitness: any = {
		...baseWitness,
		evals: averagedEvals,
	}

	for (const [prop, values] of propValues) {
		averagedWitness[prop] =
			values.length > 0
				? values.reduce((sum, val) => sum + val, 0) / values.length
				: baseWitness[prop as keyof WitnessV2]
	}

	return [averagedWitness, latestTimestamp]
}

function calculateOptimalStep(
	currentLength: number,
	maxLength: number,
	minStep: number
): number {
	if (currentLength <= maxLength) return minStep

	let step = minStep
	while (Math.ceil(currentLength / step) > maxLength) {
		step *= 2
	}
	return step
}

type CurrentFormat = V2

function migrateFromV0ToV1(_: V0): V1 {
	throw new Error("Not implemented, we don't have any V0 data anymore")
}

function migrateFromV1ToV2(dataV1: V1): V2 {
	for (const [_runId, runV1] of dataV1.runs) {
		for (const historyV1 of runV1) {
			const allWitnessUpdates = historyV1.witnessUpdates
			const historyV2 = historyV1 as unknown as RunHistoryV2

			delete (historyV2 as { witnessUpdates?: any }).witnessUpdates

			historyV2.sampledWitnessUpdates = allWitnessUpdates.slice()
			historyV2.lastFewWitnessUpdates = allWitnessUpdates.slice()

			// cleanup bad values in LR, if exists
			historyV2.observedLrByStep = historyV2.observedLrByStep.filter((s) =>
				isGoodNumber(s[1])
			)

			// cleanup witness history :)
			cleanupWitnessUpdates(historyV2)
		}
	}
	return dataV1 as unknown as V2
}

const migrations: Record<Version, (data: any) => CurrentFormat> = {
	unversioned: (data: V0) => migrateFromV1ToV2(migrateFromV0ToV1(data)),
	'1': (data: V1) => migrateFromV1ToV2(data),
	'2': (data: V2) => data,
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

type WitnessV1 = WitnessV2
type RunHistoryV1 = Omit<
	RunHistoryV2,
	'lastFewWitnessUpdates' | 'sampledWitnessUpdates' | 'sampledWitnessStep'
> & {
	witnessUpdates: Array<[WitnessV1, any]>
}
interface V1 {
	lastUpdateInfo: LastUpdateInfo
	runs: Map<string, RunHistoryV1[]>
	programId: PublicKey
}

interface V2 {
	lastUpdateInfo: LastUpdateInfo
	runs: Map<string, RunHistoryV2[]>
	programId: PublicKey
}

function tryMigrate(version: Version, data: any): CurrentFormat {
	console.log('Current coordinator DB version is', CURRENT_VERSION)
	console.log('Loaded coordinator DB version is', version)
	console.log(`Migrating from ${version} to ${CURRENT_VERSION}!!`)
	const migratedData = migrations[version](data)
	return migratedData
}
