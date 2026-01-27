import { Pubkey, RpcHttp, rpcHttpFindAccountTransactions } from "solana-kiss";
import { CrawlerCheckpoint, CrawlerTransaction } from "./CrawlerTypes";

export async function crawlerCycle(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  initialCheckpoint: CrawlerCheckpoint,
): Promise<{
  updatedCheckpoint: CrawlerCheckpoint;
  discoveredTransactions: Array<CrawlerTransaction>;
}> {
  const updatedCheckpoint = initialCheckpoint.map((chunk) => ({ ...chunk }));
  const newerChunkIndex =
    Math.floor(Math.random() * (updatedCheckpoint.length + 1)) - 1;
  const olderChunkIndex = newerChunkIndex + 1;
  const newerChunk = updatedCheckpoint[newerChunkIndex];
  const olderChunk = updatedCheckpoint[olderChunkIndex];
  const { newToOldTransactionsHandles } = await rpcHttpFindAccountTransactions(
    rpcHttp,
    programAddress,
    maxTransactionPerCycle,
    {
      startBeforeTransactionHandle:
        newerChunk?.oldestTransaction.transactionHandle,
      rewindUntilTransactionHandle:
        olderChunk?.newestTransaction.transactionHandle,
    },
  );
  if (newToOldTransactionsHandles.length === 0) {
    return { updatedCheckpoint, discoveredTransactions: [] };
  }
  const newerTransaction = {
    transactionHandle: newToOldTransactionsHandles[0]!,
    transactionOrdinal: newerChunk
      ? newerChunk.oldestTransaction.transactionOrdinal - 1n
      : BigInt(Math.floor(Date.now())) * maxTransactionPerMillisecond,
  };
  let olderTransaction = {
    transactionHandle:
      newToOldTransactionsHandles[newToOldTransactionsHandles.length - 1]!,
    transactionOrdinal:
      newerTransaction.transactionOrdinal -
      BigInt(newToOldTransactionsHandles.length - 1),
  };
  let transactionCounter = newToOldTransactionsHandles.length;
  if (
    olderTransaction.transactionHandle ===
    olderChunk?.newestTransaction.transactionHandle
  ) {
    olderTransaction = olderChunk.oldestTransaction;
    transactionCounter += olderChunk.transactionCounter - 1;
    updatedCheckpoint.splice(olderChunkIndex, 1);
    newToOldTransactionsHandles.pop();
  }
  if (newerChunk !== undefined) {
    newerChunk.oldestTransaction = olderTransaction;
    newerChunk.transactionCounter += transactionCounter;
  } else {
    updatedCheckpoint.unshift({
      newestTransaction: newerTransaction,
      oldestTransaction: olderTransaction,
      transactionCounter: transactionCounter,
    });
  }
  if (newToOldTransactionsHandles.length === 0) {
    return { updatedCheckpoint, discoveredTransactions: [] };
  }
  const discoveredTransactions = newToOldTransactionsHandles.map(
    (transactionHandle, transactionIndex) => ({
      transactionHandle,
      transactionOrdinal:
        newerTransaction.transactionOrdinal - BigInt(transactionIndex),
    }),
  );
  return { updatedCheckpoint, discoveredTransactions };
}

const maxTransactionPerCycle = 100;
const maxTransactionPerMillisecond = 1000n;
