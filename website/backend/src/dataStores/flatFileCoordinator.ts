import { readFileSync } from 'fs'
import { writeFile } from 'fs/promises'
import path from 'path'
import {
	CoordinatorConfig,
	Model,
	PsycheCoordinator,
	RunMetadata,
	lr_at_step,
} from 'psyche-deserialize-zerocopy-wasm'
import {
	psycheJsonReviver,
	psycheJsonReplacer,
	RunSummary,
	RunData,
	Metrics,
	OverTime,
	ChainTimestamp,
	getRunPDA,
	RunRoundClient,
	TxSummary,
} from 'shared'
import { CoordinatorDataStore, LastUpdateInfo } from '../dataStore.js'
import { WitnessMetadata, WitnessEvalResult } from '../idlTypes.js'
import { PublicKey } from '@solana/web3.js'
import { isClientWitness } from '../witness.js'
import EventEmitter from 'events'
import { UniqueRunKey, runKey } from '../coordinator.js'

type Witness = Omit<WitnessMetadata, 'evals'> & {
	evals: Array<{
		name: string
		value: number
	}>
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

	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
	witnessUpdates: Array<[Witness, ChainTimestamp]>
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
			const { lastUpdateInfo, runs, programId } = JSON.parse(
				readFileSync(this.#db, 'utf-8'),
				psycheJsonReviver
			)
			if (this.#programId.equals(programId)) {
				this.#lastUpdateInfo = lastUpdateInfo
				this.#runs = runs
				console.log(
					`loaded DB from disk at slot ${this.#lastUpdateInfo.highestSignature?.slot ?? 0}`
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
		await writeFile(
			this.#db,
			JSON.stringify(
				{
					lastUpdateInfo: this.#lastUpdateInfo,
					runs: this.#runs,
					programId: this.#programId,
				},
				psycheJsonReplacer
			)
		)
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
			witnessUpdates: [],
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

	witnessRun(
		pubkey: string,
		witness: WitnessMetadata,
		timestamp: ChainTimestamp
	) {
		const runs = this.#runs.get(pubkey)
		const lastRun = runs?.at(-1)
		if (!runs || !lastRun) {
			throw new Error(
				`Tried to get run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}
		// we don't reallllllly care if it's shut down.
		lastRun.lastUpdated = timestamp

		// format evals to nice strings to save tons of space
		const { evals, ...restWitness } = witness

		// could be a bigint, could be a BN, kind of annoying. TODO fix somewhere else.
		const l =
			typeof evals.len === 'object' && evals.len && 'toNumber' in evals.len
				? evals.len.toNumber()
				: Number(evals.len)
		const fixedEvals = []
		for (const { name, value } of evals.data.slice(
			0,
			l
		) as WitnessEvalResult[]) {
			const firstZero = name[0].findIndex((v) => v === 0)
			const nameStr = Buffer.from(name[0].slice(0, firstZero)).toString('utf-8')
			fixedEvals.push({
				name: nameStr,
				value,
			})
		}
		lastRun.witnessUpdates.push([
			{ ...restWitness, evals: fixedEvals },
			timestamp,
		])

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
		const runs = rawRuns.map((r) => r[0]).filter((r) => !!r)
		const summaries = {
			runs,
			totalTokens: runs.reduce((sum, run) => sum + run.completedTokens, 0n),
			totalTokensPerSecondActive: rawRuns.reduce((sum, [summary, run]) => {
				const ACTIVE_TIMEOUT_MS = 10 * 60 * 1000
				if (
					summary?.status.type !== 'active' ||
					Date.now() - run.lastUpdated.time.getTime() > ACTIVE_TIMEOUT_MS
				) {
					return sum
				}
				const lastWitness = run.witnessUpdates.at(-1)
				if (!lastWitness) {
					return sum
				}
				return sum + BigInt(Math.round(lastWitness[0].tokens_per_sec))
			}, 0n),
		}
		this.#summaryCache = summaries
		return summaries
	}

	getNumRuns(): number {
		return [...this.#runs.values()].reduce(
			(sum, runs) => sum + runs.filter((r) => r.lastState).length,
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

		const numSamples = 1000

		const linearWitnessHistory = chopOffDivergentHistory(
			run.witnessUpdates.map((w) => [w[0].step, w[0]] as const)
		)

		const evals: Record<
			string,
			Array<readonly [step: number, value: number]>
		> = {}
		for (const [step, r] of linearWitnessHistory) {
			for (const { name, value } of r.evals) {
				if (!(name in evals)) {
					evals[name] = []
				}
				evals[name].push([step, value] as const)
			}
		}
		for (const evalName in evals) {
			evals[evalName] = fairSample(
				averageSameStepValues(evals[evalName]),
				numSamples
			)
		}
		const history: OverTime<Metrics> = {
			bandwidth: fairSample(
				averageSameStepValues(
					linearWitnessHistory
						.map(([step, h]) => [step, h.bandwidth_per_sec] as const)
						.filter(goodNumber)
				),
				numSamples
			),
			loss: fairSample(
				averageSameStepValues(
					linearWitnessHistory
						.map(([step, h]) => [step, h.loss] as const)
						.filter(goodNumber)
				),
				numSamples
			),
			tokensPerSecond: fairSample(
				averageSameStepValues(
					linearWitnessHistory
						.map(([step, h]) => [step, h.tokens_per_sec] as const)
						.filter(goodNumber)
				),
				numSamples
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
	const currentStep = BigInt(c.progress.step)
	const totalSteps = BigInt(c.config.total_steps)

	const completedTokens = calculateTokens(
		currentStep,
		tokensPerSequence,
		batchSizeStart,
		batchSizeEnd,
		warmupTokens
	)

	const totalTokens = calculateTokens(
		totalSteps,
		tokensPerSequence,
		batchSizeStart,
		batchSizeEnd,
		warmupTokens
	)

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
		startTime: run.createdAt,
		pauseHistory: run.pauseTimestamps,
		totalTokens,
		completedTokens,
		size: run.lastState.metadata.num_parameters,
		type: 'text', // TODO add type / tags? :)
	}
	return summary
}

// linear warmup then constant at the warmup-end value is just a piecewise of area of trapezoid and area of rectangle!
function calculateTokens(
	step: bigint,
	tokensPerSequence: bigint,
	batchSizeStart: bigint,
	batchSizeEnd: bigint,
	warmupTokens: bigint
) {
	const avgBatchSizeDuringWarmup = (batchSizeStart + batchSizeEnd) / 2n

	// avoid div by 0
	if (tokensPerSequence === 0n || avgBatchSizeDuringWarmup === 0n) {
		return 0n
	}
	const stepsForWarmup =
		warmupTokens / (tokensPerSequence * avgBatchSizeDuringWarmup)

	// trapezoid area (warmup phase)
	const trapezoidTokens =
		((step < stepsForWarmup ? step : stepsForWarmup) *
			tokensPerSequence *
			(batchSizeStart + batchSizeEnd)) /
		2n

	// rectangle area (post-warmup phase)
	const desiredPostWarmupSteps = step - stepsForWarmup
	const postWarmupSteps =
		desiredPostWarmupSteps > 0 ? desiredPostWarmupSteps : 0n
	const rectangleTokens = postWarmupSteps * tokensPerSequence * batchSizeEnd

	return trapezoidTokens + rectangleTokens
}

function averageSameStepValues(
	values: Array<readonly [number, number]>
): Array<readonly [number, number]> {
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

// sample n items, always including the first and last items.
function fairSample<T>(array: T[], sampleSize: number) {
	const length = array.length

	if (length === 0) return []

	if (sampleSize >= length || sampleSize <= 2) {
		return [...array]
	}

	const result = [array[0]]

	const step = (length - 1) / (sampleSize - 1)

	for (let i = 1; i < sampleSize - 1; i++) {
		const index = Math.round(i * step)
		result.push(array[index])
	}

	result.push(array[length - 1])

	return result
}

/**
 * Given an array of
 * `const values: Array<[x: number, y: number]>`
 * Detects if x ever goes backwards, and then chops off that branch,
 * so with a bunch of divergent branches linearly flattened,
 * we only keep one linear branch.
 */
function chopOffDivergentHistory<T>(
	values: Array<readonly [x: number, y: T]>
): Array<readonly [x: number, y: T]> {
	const result: Array<readonly [x: number, y: T]> = []
	let maxX = -1
	for (const [step, value] of values) {
		if (step < maxX) {
			// find the divergent point - the last entry that has x < step
			const divergentIndex = result.findLastIndex(([x]) => x < step)

			// slice off all results after the divergent point
			result.length = divergentIndex + 1
		}

		result.push([step, value])
		maxX = step
	}
	return result
}
