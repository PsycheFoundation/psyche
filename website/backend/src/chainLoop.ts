import {
	BorshInstructionCoder,
	Idl,
	Program,
	Provider,
} from '@coral-xyz/anchor'
import {
	ParsedTransactionWithMeta,
	PartiallyDecodedInstruction,
	PublicKey,
} from '@solana/web3.js'
import { ChainDataStore } from './dataStore.js'
import ProgramEventListener from './programEventListener.js'

export function startWatchChainLoop<D>(): <
	T,
	I extends Idl,
	S extends ChainDataStore,
>(
	name: string,
	dataStore: S,
	program: Program<I>,
	websocketRpcUrl: string,
	minimumSlotToIndexFrom: number,
	cancelled: { cancelled: boolean },
	onError: (error: unknown) => void,
	process: {
		onStartCatchup(firstStateEver: boolean): T
		onInstruction(
			tx: ParsedTransactionWithMeta,
			instruction: PartiallyDecodedInstruction,
			decoded: D,
			state: T
		): void
		onDoneCatchup(dataStore: S, state: T): Promise<void>
	},
	delayMsBetweenUpdates?: number
) => Promise<void> {
	return async (
		name,
		dataStore,
		program,
		websocketRpcUrl,
		minimumSlotToIndexFrom,
		cancelled,
		onError,
		process,
		delayMsBetweenUpdates = 1000
	) => {
		const instructionCoder = new BorshInstructionCoder(program.rawIdl)

		let wsListener: ProgramEventListener | null = null

		while (!cancelled.cancelled) {
			try {
				let lastUpdate = dataStore.lastUpdate()
				console.error(`[${name}] chain loop starting up`)

				wsListener = new ProgramEventListener(
					websocketRpcUrl,
					program.programId,
					name
				)

				while (!cancelled.cancelled) {
					await Promise.race([
						wsListener.nextUpdate(),
						new Promise((r) => setTimeout(r, delayMsBetweenUpdates)),
					])

					const startSlot = Math.max(
						minimumSlotToIndexFrom,
						lastUpdate?.highestSignature?.slot ?? 0
					)
					const catchupTxs = await catchupOnTxsToAddress(
						name,
						program.provider,
						program.programId,
						startSlot,
						cancelled
					)

					if (catchupTxs === null) {
						// cancelled
						return
					}

					if (catchupTxs.length) {
						console.debug(
							`[${name}] updated from slot ${startSlot} to latest slot.`
						)
					}

					const state = process.onStartCatchup(!lastUpdate.highestSignature)

					for (const data of catchupTxs) {
						const tx = data[1]
						const index = catchupTxs.indexOf(data)
						if (index % 1000 === 1) {
							console.log(
								`[${name}] processing changes from tx ${index} / ${catchupTxs.length} (${((index / catchupTxs.length) * 100).toFixed(2)}%)`
							)
						}
						if (cancelled.cancelled) {
							return
						}
						const instr = [
							...tx.transaction.message.instructions,
							...(data[1].meta?.innerInstructions?.flatMap(
								(i) => i.instructions
							) ?? []),
						]
						for (const i of instr) {
							// instructions without a payload aren't useful to us.
							if (!('data' in i)) {
								continue
							}
							let rawDecoded
							try {
								rawDecoded = instructionCoder.decode(i.data, 'base58')
							} catch (err) {
								// instructions that we can't decode with our IDL aren't useful to us. hopefully this doesn't break.
								console.warn(
									`Failed to process instruction from tx in slot ${tx.slot}. Attempting to continue. Maybe we have a different IDL than this was created with?`
								)
							}
							if (!rawDecoded) {
								console.warn(
									`No data for instruction with data${i.data}. Skipping.`
								)
								continue
							}
							try {
								// this is a bit of a "hope and pray" cast,
								// the IDL stuff is a bit of a nightmare to work with.
								process.onInstruction(tx, i, rawDecoded as D, state)
								if (cancelled.cancelled) {
									break
								}
							} catch (err) {
								throw new Error(
									`failed to process instruction in ${program.programId} from TX in slot ${tx.slot}: ${err}`
								)
							}
						}
					}
					if (cancelled.cancelled) {
						break
					}

					lastUpdate.time = new Date()
					if (catchupTxs.length) {
						const [signature, { slot }] = catchupTxs.at(-1)!
						lastUpdate.highestSignature = {
							signature,
							slot,
						}
					}
					await process.onDoneCatchup(dataStore, state)
					await dataStore.sync(lastUpdate)
					console.log(`[${name}] written to disk.`)
				}
			} catch (err) {
				console.error(
					`[${name}] chain loop encountered an error, restarting,`,
					err
				)
				onError(err)
			} finally {
				if (wsListener) {
					wsListener.disconnect()
					wsListener = null
				}
			}
			if (cancelled.cancelled) {
				console.info(`[${name}] chain loop was cancelled, exiting cleanly...`)
			}
		}
	}
}

async function catchupOnTxsToAddress(
	name: string,
	provider: Provider,
	address: PublicKey,
	lastIndexedSlot: number,
	cancelled: { cancelled: boolean }
) {
	// start with the newest possible signature, and go back to the oldest one.
	let oldestSeenSignature: { signature: string; slot: number } | undefined =
		undefined
	const allSignatures = []
	while (true) {
		console.log(
			`[${name}] fetching sigs from slot ${lastIndexedSlot} to ${oldestSeenSignature?.slot ?? 'the latest block'}: total ${allSignatures.length}`
		)
		const signatures = await provider.connection.getSignaturesForAddress(
			address,
			{
				before: oldestSeenSignature?.signature,
			},
			'confirmed'
		)

		if (cancelled.cancelled) {
			return null
		}

		const signaturesAfterLastProcessedSlot = signatures.filter(
			(s) => s.slot > lastIndexedSlot
		)

		// pick the last signature as our next iter's highest sig to start from,
		// so we'll iter further and further back into the past
		oldestSeenSignature = signaturesAfterLastProcessedSlot.at(-1)
		allSignatures.push(
			...signaturesAfterLastProcessedSlot.map((s) => s.signature)
		)

		// if we've run out of signatures to process, or at lesat one signature has gone past our last processed slot, we're done.
		if (
			signatures.length === 0 ||
			signaturesAfterLastProcessedSlot.length != signatures.length
		) {
			break
		}
	}

	// nice early exit :)
	if (allSignatures.length === 0) {
		return []
	}

	console.log(
		`[${name}] fetching ${allSignatures.length} transactions catching up from slot ${lastIndexedSlot} to ${oldestSeenSignature?.slot ?? 'the latest block'}`
	)

	let completedCount = 0

	// fetch in parallel. the fetch rate limiter will limit this to something reasonable.
	const fetchPromises = allSignatures.map((signature) =>
		provider.connection
			.getParsedTransaction(signature, {
				commitment: 'confirmed',
				maxSupportedTransactionVersion: 0,
			})
			.then((tx) => {
				completedCount++
				console.log(
					`[${name}] fetched sig ${completedCount}/${allSignatures.length}, ${((completedCount / allSignatures.length) * 100).toFixed(2)}% ...`
				)

				if (cancelled.cancelled) {
					throw new Error('Operation cancelled')
				}

				if (!tx) {
					console.warn(
						`[${name}] Transaction failed to decode at signature ${signature}`
					)
				}

				return [signature, tx] as const
			})
	)

	const transactions: Array<
		readonly [string, ParsedTransactionWithMeta | null]
	> = await Promise.all(fetchPromises)

	const catchupTxs = transactions.filter(
		(t): t is readonly [string, ParsedTransactionWithMeta] => !!t[1]
	)
	// ok here, we now have a list of TXs to the this account, from newest to oldest.
	// reverse into oldest -> newest, so can process in chronological order.
	catchupTxs.reverse()

	return catchupTxs
}
