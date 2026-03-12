import { Program, Provider } from '@coral-xyz/anchor'
import { ParsedTransactionWithMeta, PublicKey } from '@solana/web3.js'
import {
	ChainTimestamp,
	getRunPDA,
	PsycheSolanaCoordinator,
	TxSummary,
} from 'shared'
import {
	load_coordinator_from_bytes,
	PsycheCoordinator,
} from 'psyche-deserialize-zerocopy-wasm'
import {
	PsycheCoordinatorInstructionsUnion,
	WitnessMetadata,
} from './idlTypes.js'
import { CoordinatorDataStore } from './dataStore.js'
import { startWatchChainLoop } from './chainLoop.js'
import { makeRetryPromise } from './rateLimit.js'

interface RunUpdates {
	created?: {
		runId: string
		timestamp: ChainTimestamp
	}
	destroyedAt?: ChainTimestamp
	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
	witnessUpdates: Array<[WitnessMetadata, ChainTimestamp]>
	lastUpdated: {
		coordinatorAccountAddress: string
		timestamp: ChainTimestamp
	}
	txs: TxSummary[]
	configChanged: boolean
}

function timestampFromTx(tx: ParsedTransactionWithMeta): ChainTimestamp {
	return {
		slot: BigInt(tx.slot),
		// solana timestamps are in second, pad out to ms.
		time: new Date(+(tx.blockTime?.toString().padEnd(13, '0') ?? Date.now())),
	}
}

function txSummary(
	tx: ParsedTransactionWithMeta,
	decoded: { name: string; data: any }
): TxSummary {
	return {
		method: decoded.name,
		data: JSON.stringify(decoded.data),
		pubkey: tx.transaction.message.accountKeys[0].pubkey.toString(),
		timestamp: timestampFromTx(tx),
		txHash: tx.transaction.signatures[0],
	}
}

// When any change is made to a run, we'll insert the run info into its existing entry here, if it's not "destroyed"
// or, if it was destroyed, we mark it as "destroyed".

// If the last run in the list for a given key has not been destroyed,
// we'll fetch its state on-chain.

// This should ascribe witness updates, pauses, etc correctly to each run.
class UpdateManager {
	#runUpdates: Map<string, [RunUpdates, ...RunUpdates[]]> = new Map()

	createNewRun({
		runPdaAddr,
		runId,
		coordinatorAddr,
		decoded,
		tx,
	}: {
		runPdaAddr: string
		runId: string
		coordinatorAddr: string
		decoded: { name: string; data: any }
		tx: ParsedTransactionWithMeta
	}) {
		const runsAtThisAddress = this.#runUpdates.get(runPdaAddr)
		const existingRun = runsAtThisAddress?.at(-1)

		if (existingRun && !existingRun.destroyedAt) {
			throw new Error(
				`Can't create a new run; run at ${runPdaAddr} was *not* destroyed, but we're trying to create a new one!`
			)
		}

		const timestamp = timestampFromTx(tx)

		// either there's nothing in the list, or the last run was destroyed
		// so we make a new one, track it, and return it
		const newRun: RunUpdates = {
			created: {
				runId,
				timestamp,
			},
			lastUpdated: { timestamp, coordinatorAccountAddress: coordinatorAddr },
			pauseTimestamps: [],
			witnessUpdates: [],
			configChanged: false,
			txs: [txSummary(tx, decoded)],
		}
		if (runsAtThisAddress) {
			runsAtThisAddress.push(newRun)
		} else {
			this.#runUpdates.set(runPdaAddr, [newRun])
		}
		return newRun
	}

	getAndTouchCurrentRun({
		runPdaAddr,
		coordinatorAddr,
		decoded,
		tx,
	}: {
		runPdaAddr: string
		coordinatorAddr: string
		decoded: { name: string; data: any }
		tx: ParsedTransactionWithMeta
	}) {
		const timestamp = timestampFromTx(tx)

		// ensure the runs array is set
		if (!this.#runUpdates.has(runPdaAddr)) {
			this.#runUpdates.set(runPdaAddr, [
				{
					pauseTimestamps: [],
					witnessUpdates: [],
					lastUpdated: {
						timestamp,
						coordinatorAccountAddress: coordinatorAddr,
					},
					configChanged: false,
					txs: [],
				},
			])
		}

		const runsAtThisAddress = this.#runUpdates.get(runPdaAddr)!
		const existingRun = runsAtThisAddress.at(-1)!

		if (existingRun.destroyedAt) {
			throw new Error(
				`Run at ${runPdaAddr} was destroyed and a new one hasn't been created`
			)
		}
		if (existingRun.lastUpdated.coordinatorAccountAddress !== coordinatorAddr) {
			throw new Error(
				`actual run addr is wrongggggg. got ${coordinatorAddr}, expected ${existingRun.lastUpdated.coordinatorAccountAddress}`
			)
		}
		existingRun.lastUpdated = {
			timestamp,
			coordinatorAccountAddress: coordinatorAddr,
		}
		existingRun.txs.push(txSummary(tx, decoded))

		return existingRun
	}

	getRuns(): MapIterator<[string, [RunUpdates, ...RunUpdates[]]]> {
		return this.#runUpdates.entries()
	}
}

const COORDINATOR_LOOP_DELAY_MS = 5000

export async function startWatchCoordinatorChainLoop(
	dataStore: CoordinatorDataStore,
	coordinator: Program<PsycheSolanaCoordinator>,
	websocketRpcUrl: string,
	minSlot: number,
	cancelled: { cancelled: boolean },
	onError: (error: unknown) => void
) {
	const getCoordinatorStateWithRetries = makeRetryPromise(
		getRunCoordinatorState
	)

	await startWatchChainLoop<PsycheCoordinatorInstructionsUnion>()(
		'coordinator',
		dataStore,
		coordinator,
		websocketRpcUrl,
		minSlot,
		cancelled,
		onError,
		{
			onStartCatchup() {
				return new UpdateManager()
			},
			onInstruction(tx, i, decoded, runUpdates) {
				// `i.accounts` is an array of all the accounts used in this TX.
				// we're using it to grab the address of the specific run we're updating.
				// this is different per instruction, but fixed order based on the IDL/contract code,
				// so it's safe to hardcode the index here.
				switch (decoded.name) {
					case 'init_coordinator': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const expectedRunAddr = getRunPDA(
							coordinator.programId,
							decoded.data.params.run_id
						)
						if (runPdaAddr !== expectedRunAddr.toString()) {
							throw new Error(
								`Expected run addr ${expectedRunAddr.toString()}, but saw run addr ${runPdaAddr}`
							)
						}
						runUpdates.createNewRun({
							runPdaAddr,
							runId: decoded.data.params.run_id,
							coordinatorAddr,
							tx,
							decoded,
						})
						break
					}

					case 'update': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const r = runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						// instead of pulling the config from solana, where the deserialization is.. weird about enums
						// we just mark it changed, and pull it nicely later with wasm
						r.configChanged = true
						break
					}
					case 'tick': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'join_run': {
						const runPdaAddr = i.accounts[2].toString()
						const coordinatorAddr = i.accounts[3].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'set_paused': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const run = runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						run.pauseTimestamps.push([
							decoded.data.paused ? 'paused' : 'unpaused',
							run.lastUpdated.timestamp,
						])
						break
					}
					case 'free_coordinator': {
						const runPdaAddr = i.accounts[2].toString()
						const coordinatorAddr = i.accounts[3].toString()
						const run = runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						run.destroyedAt = run.lastUpdated.timestamp
						break
					}
					case 'witness': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const run = runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						run.witnessUpdates.push([
							decoded.data.metadata,
							run.lastUpdated.timestamp,
						])
						break
					}
					case 'checkpoint': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'health_check': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'set_future_epoch_rates': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'warmup_witness': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'cooldown_witness': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					case 'update_client_version': {
						const runPdaAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun({
							runPdaAddr,
							coordinatorAddr,
							decoded,
							tx,
						})
						break
					}
					default: {
						const _missed_tx: never = decoded
						throw new Error(
							`Unexpected instruction ${JSON.stringify(_missed_tx)} at slot ${
								tx.slot
							} ${JSON.stringify(decoded)}`
						)
					}
				}
			},

			async onDoneCatchup(store, runUpdates) {
				const allRuns = [...runUpdates.getRuns()].filter((run) => {
					// known broken addrs
					if (
						run[0] === '5rHDPfLLYNxKDh86GYtx6nF94WAS2uvHp4zp7WUqsBSg' ||
						run[0] === 'Gewx2auE9MBxNkbDevDSPBdDjDekNhhoyYyYaxEfM2gy' ||
						run[0] === 'GWBFVhEi1nCp2J2E7paukFMZ9PE1hAa9dQcvZSeQoKAj'
					) {
						return false
					}
					return true
				})
				if (allRuns.length === 0) {
					return
				}
				const allRunsWithState: Array<
					readonly [
						string,
						Array<
							RunUpdates & {
								state?: [PsycheCoordinator, ChainTimestamp]
							}
						>,
					]
				> = await Promise.all(
					allRuns.map(([addr, runsAtThisAddr]) => {
						const latestRun = runsAtThisAddr.at(-1)!
						// if the run is currently destroyed, we can't fetch a state update for it,
						// since it's gone on-chain.
						const canFetchState = !latestRun.destroyedAt

						// this fetches the state for the last run in the list.
						if (canFetchState) {
							const { coordinatorAccountAddress } = latestRun.lastUpdated
							console.log(
								`[coordinator] fetching state for run ${addr}, whose coordinator PDA lives at ${coordinatorAccountAddress}, we saw updated at slot ${latestRun.lastUpdated.timestamp.slot}`
							)
							return getCoordinatorStateWithRetries(
								coordinator.provider,
								new PublicKey(coordinatorAccountAddress)
							).then(
								(runState) =>
									[
										addr,
										runsAtThisAddr.map((run, i) => {
											if (i === runsAtThisAddr.length - 1) {
												return {
													...run,
													state: runState,
												}
											}
											return run
										}),
									] as const
							)
						}
						return [addr, runsAtThisAddr] as const
					})
				)
				for (const [i, [pubkey, runs]] of allRunsWithState.entries()) {
					console.log(
						`[coordinator] applying update for run ${i + 1}/${
							allRunsWithState.length
						}`
					)
					for (const run of runs) {
						if (run.created) {
							store.createRun(
								pubkey,
								run.created.runId,
								run.created.timestamp,
								run.state?.[0]
							)
						}
						if (run.state) {
							store.updateRun(
								pubkey,
								run.state[0],
								run.state[1],
								run.configChanged
							)
						}
						for (const tx of run.txs) {
							store.trackTx(
								pubkey,
								tx.pubkey,
								tx.method,
								tx.data,
								tx.txHash,
								tx.timestamp
							)
						}
						store.appendRunWitnesses(pubkey, run.witnessUpdates)
						for (const [pause, timestamp] of run.pauseTimestamps) {
							store.setRunPaused(pubkey, pause === 'paused', timestamp)
						}
						if (run.destroyedAt) {
							store.destroyRun(pubkey, run.destroyedAt)
						}
					}
				}
			},
		},
		COORDINATOR_LOOP_DELAY_MS
	)
}

async function getRunCoordinatorState(
	provider: Provider,
	runPubkey: PublicKey
): Promise<[PsycheCoordinator, ChainTimestamp]> {
	const accountInfo = await provider.connection.getParsedAccountInfo(
		runPubkey,
		'confirmed'
	)
	if (!accountInfo.value?.data) {
		throw new Error('No data for run at address: ' + runPubkey)
	}

	if (!(accountInfo.value.data instanceof Buffer)) {
		throw new Error(
			'Data is not a buffer when loading run at address: ' + runPubkey
		)
	}
	try {
		const state = load_coordinator_from_bytes(accountInfo.value.data)
		return [
			state,
			{
				slot: BigInt(accountInfo.context.slot),
				time: new Date(),
			},
		]
	} catch (err) {
		throw new Error(`Failed to deserialize run at address ${runPubkey}: ${err}`)
	}
}
