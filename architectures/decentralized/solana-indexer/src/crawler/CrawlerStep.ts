import { Pubkey, RpcHttp, rpcHttpFindAccountTransactions } from "solana-kiss";
import { CrawlerCheckpoint, CrawlerTransaction } from "./CrawlerTypes";

export async function crawlerStep(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  initialCheckpoint: CrawlerCheckpoint,
): Promise<{
  updatedCheckpoint: CrawlerCheckpoint;
  discoveredTransactions: Array<CrawlerTransaction>;
}> {
  const updatedCheckpoint = initialCheckpoint.map((step) => ({ ...step }));
  const newerStepIndex =
    Math.floor(Math.random() * (updatedCheckpoint.length + 1)) - 1;
  const olderStepIndex = newerStepIndex + 1;
  const newerStep = updatedCheckpoint[newerStepIndex];
  const olderStep = updatedCheckpoint[olderStepIndex];
  const { newToOldTransactionsHandles } = await rpcHttpFindAccountTransactions(
    rpcHttp,
    programAddress,
    mexTransactionPerStep,
    {
      startBeforeTransactionHandle:
        newerStep?.oldestTransaction.transactionHandle,
      rewindUntilTransactionHandle:
        olderStep?.newestTransaction.transactionHandle,
    },
  );
  if (newToOldTransactionsHandles.length === 0) {
    return { updatedCheckpoint, discoveredTransactions: [] };
  }
  const newerTransaction = {
    transactionHandle: newToOldTransactionsHandles[0]!,
    transactionOrdinal: newerStep
      ? newerStep.oldestTransaction.transactionOrdinal
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
    olderStep?.newestTransaction.transactionHandle
  ) {
    olderTransaction = olderStep.oldestTransaction;
    transactionCounter += olderStep.transactionCounter - 1;
    updatedCheckpoint.splice(olderStepIndex, 1);
    newToOldTransactionsHandles.pop();
  }
  if (newerStep !== undefined) {
    newerStep.oldestTransaction = olderTransaction;
    newerStep.transactionCounter += transactionCounter;
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

const mexTransactionPerStep = 100;
const maxTransactionPerMillisecond = 1000n;
