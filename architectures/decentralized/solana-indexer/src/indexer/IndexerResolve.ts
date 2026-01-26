import {
  ErrorStack,
  Pubkey,
  rpcHttpWaitForTransaction,
  Solana,
  timeoutMs,
} from "solana-kiss";
import { CrawlerTransaction } from "../crawler/CrawlerTypes";
import { utilLogWithTimestamp, utilRunInParallel } from "../utils";
import { indexerParse } from "./IndexerParse";

export async function indexerResolve(
  solana: Solana,
  programAddress: Pubkey,
  crawlerTransactions: Array<CrawlerTransaction>,
) {
  const tasks = await utilRunInParallel(
    crawlerTransactions,
    async (crawlerTransaction) => {
      return await resolveTransaction(
        solana,
        programAddress,
        crawlerTransaction,
      );
    },
  );
  const resolvedInstructions = [];
  for (const task of tasks) {
    if (task.result.error) {
      throw new ErrorStack(
        `Failed to resolve transaction: ${task.input.transactionHandle}`,
        task.result.error,
      );
    }
    if (task.result.value) {
      resolvedInstructions.push(...task.result.value);
    }
  }
  return resolvedInstructions;
}

async function resolveTransaction(
  solana: Solana,
  programAddress: Pubkey,
  crawlerTransaction: CrawlerTransaction,
) {
  const { transactionExecution, transactionFlow } =
    await rpcHttpWaitForTransaction(
      solana.getRpcHttp(),
      crawlerTransaction.transactionHandle,
      async (context) => {
        if (context.totalDurationMs >= 10 * 1000) {
          utilLogWithTimestamp(
            programAddress,
            `Giving up waiting for transaction: ${crawlerTransaction.transactionHandle}`,
            context.totalDurationMs,
          );
          return false;
        }
        await timeoutMs(context.retriedCounter * 1000);
        return true;
      },
    );
  if (transactionExecution.transactionError !== null) {
    return [];
  }
  if (transactionFlow === undefined) {
    return [];
  }
  return await indexerParse(
    solana,
    programAddress,
    transactionExecution.blockTime,
    crawlerTransaction.transactionOrdinal,
    transactionFlow,
  );
}
