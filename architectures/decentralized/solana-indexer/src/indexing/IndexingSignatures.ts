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
	beginCheckpoint: IndexingCheckpoint,
	onChunk: (
		updatedCheckpoint: IndexingCheckpoint,
		transactionsInfos: Array<{
			transactionId: Signature
			transactionOrdinal: bigint
		}>
	) => Promise<void>
): Promise<never> {
	const orderedIndexedChunks = beginCheckpoint.orderedIndexedChunks.map(
		(c) => ({ ...c })
	)
	while (true) {
		try {
			await indexingSignaturesChunk(
				rpcHttp,
				programAddress,
				orderedIndexedChunks,
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
	orderedIndexedChunks: Array<IndexingCheckpointChunk>,
	onChunk: (
		updatedCheckpoint: IndexingCheckpoint,
		transactionsInfos: Array<{
			transactionId: Signature
			transactionOrdinal: bigint
		}>
	) => Promise<void>
) {
	const newerChunkIndex =
		Math.floor(Math.random() * (orderedIndexedChunks.length + 1)) - 1
	const olderChunkIndex = newerChunkIndex + 1
	const newerChunkInfo = orderedIndexedChunks[newerChunkIndex]
	const olderChunkInfo = orderedIndexedChunks[olderChunkIndex]
	const { backwardTransactionsIds } = await rpcHttpFindAccountTransactions(
		rpcHttp,
		programAddress,
		1000,
		{
			startBeforeTransactionId: newerChunkInfo?.oldestTransactionId,
			rewindUntilTransactionId: olderChunkInfo?.newestTransactionId,
		}
	)
	if (backwardTransactionsIds.length === 0) {
		return
	}
	const newerTransactionOrdinal = newerChunkInfo
		? newerChunkInfo.oldestTransactionOrdinal
		: BigInt(Math.floor(new Date().getTime())) * maxTransactionPerMillisecond
	let olderTransactionOrdinal =
		newerTransactionOrdinal - BigInt(backwardTransactionsIds.length)
	let transactionCounter = backwardTransactionsIds.length
	const newerTransactionId = backwardTransactionsIds[0]!
	let olderTransactionId =
		backwardTransactionsIds[backwardTransactionsIds.length - 1]!
	if (olderTransactionId === olderChunkInfo?.newestTransactionId) {
		olderTransactionId = olderChunkInfo.oldestTransactionId
		olderTransactionOrdinal = olderChunkInfo.oldestTransactionOrdinal
		transactionCounter += olderChunkInfo.transactionCounter - 1
		orderedIndexedChunks.splice(olderChunkIndex, 1)
		backwardTransactionsIds.pop()
	}
	if (newerChunkInfo !== undefined) {
		newerChunkInfo.oldestTransactionId = olderTransactionId
		newerChunkInfo.oldestTransactionOrdinal = olderTransactionOrdinal
		newerChunkInfo.transactionCounter += transactionCounter
	} else {
		orderedIndexedChunks.unshift({
			newestTransactionId: newerTransactionId,
			oldestTransactionId: olderTransactionId,
			newestTransactionOrdinal: newerTransactionOrdinal,
			oldestTransactionOrdinal: olderTransactionOrdinal,
			transactionCounter: transactionCounter,
		})
	}
	if (backwardTransactionsIds.length === 0) {
		return
	}
	const updatedCheckpoint = {
		orderedIndexedChunks: orderedIndexedChunks.map((c) => ({ ...c })),
	}
	const transactionInfos = backwardTransactionsIds.map(
		(transactionId, index) => ({
			transactionId,
			transactionOrdinal: newerTransactionOrdinal - BigInt(index),
		})
	)
	await onChunk(updatedCheckpoint, transactionInfos)
}

const maxTransactionPerMillisecond = 1000n
