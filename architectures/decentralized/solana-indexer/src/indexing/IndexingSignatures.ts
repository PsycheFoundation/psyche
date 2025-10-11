import {
	Pubkey,
	RpcHttp,
	rpcHttpFindAccountTransactions,
	Signature,
} from 'solana-kiss'
import {
	IndexingCheckpoint,
	IndexingCheckpointChunk,
} from './IndexingCheckpoint'

export async function indexingSignaturesLoop(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	onChunk: (
		foundHistory: Array<{
			signature: Signature
			ordering: bigint
		}>,
		updatedCheckpoint: IndexingCheckpoint
	) => Promise<void>
): Promise<never> {
	const indexedChunks = startingCheckpoint.indexedChunks.map((c) => ({ ...c }))
	while (true) {
		try {
			await indexingSignaturesChunk(
				rpcHttp,
				programAddress,
				indexedChunks,
				onChunk
			)
		} catch (error) {
			console.error('Indexing signatures loop chunk failed', error)
		}
	}
}

async function indexingSignaturesChunk(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	indexedChunks: Array<IndexingCheckpointChunk>,
	onChunk: (
		foundExecutions: Array<{ signature: Signature; ordering: bigint }>,
		updatedCheckpoint: IndexingCheckpoint
	) => Promise<void>
) {
	const prevChunkIndex =
		Math.floor(Math.random() * (indexedChunks.length + 1)) - 1
	const nextChunkIndex = prevChunkIndex + 1
	const prevChunkInfo = indexedChunks[prevChunkIndex]
	const nextChunkInfo = indexedChunks[nextChunkIndex]
	const { backwardTransactionsIds } = await rpcHttpFindAccountTransactions(
		rpcHttp,
		programAddress,
		1000,
		{
			startBeforeTransactionId: prevChunkInfo?.rewindedUntil,
			rewindUntilTransactionId: nextChunkInfo?.startedFrom,
		}
	)
	if (backwardTransactionsIds.length === 0) {
		return
	}
	const orderingHigh = prevChunkInfo
		? prevChunkInfo.orderingLow
		: BigInt(Math.floor(new Date().getTime())) *
			maxTransactionPerMillisecond *
			maxInstructionPerTransaction
	let orderingLow =
		orderingHigh -
		BigInt(backwardTransactionsIds.length) * maxInstructionPerTransaction
	let processedCounter = backwardTransactionsIds.length
	const startedFrom = backwardTransactionsIds[0]!
	let rewindedUntil =
		backwardTransactionsIds[backwardTransactionsIds.length - 1]!
	if (rewindedUntil === nextChunkInfo?.startedFrom) {
		rewindedUntil = nextChunkInfo.rewindedUntil
		orderingLow = nextChunkInfo.orderingLow
		processedCounter += nextChunkInfo.processedCounter - 1
		indexedChunks.splice(nextChunkIndex, 1)
		backwardTransactionsIds.pop()
	}
	if (prevChunkInfo !== undefined) {
		prevChunkInfo.rewindedUntil = rewindedUntil
		prevChunkInfo.orderingLow = orderingLow
		prevChunkInfo.processedCounter += processedCounter
	} else {
		indexedChunks.unshift({
			orderingHigh: orderingHigh,
			orderingLow: orderingLow,
			startedFrom: startedFrom,
			rewindedUntil: rewindedUntil,
			processedCounter: processedCounter,
		})
	}
	if (backwardTransactionsIds.length === 0) {
		return
	}
	await onChunk(
		backwardTransactionsIds.map((signature, index) => ({
			signature,
			ordering: orderingHigh - BigInt(index) * maxInstructionPerTransaction,
		})),
		{ indexedChunks: indexedChunks.map((c) => ({ ...c })) }
	)
}

const maxInstructionPerTransaction = 1000n
const maxTransactionPerMillisecond = 1000n
