import { Pubkey, Solana } from "solana-kiss";
import { crawlerLoop } from "../crawler/CrawlerLoop";
import { CrawlerCheckpoint } from "../crawler/CrawlerTypes";
import { utilLogWithTimestamp } from "../utils";
import { indexerResolve } from "./IndexerResolve";
import { IndexerInstruction } from "./IndexerTypes";

export async function indexerLoop(
  solana: Solana,
  programAddress: Pubkey,
  initialCheckpoint: CrawlerCheckpoint,
  onProgress: (params: {
    updatedCheckpoint: CrawlerCheckpoint;
    discoveredInstructions: Array<IndexerInstruction>;
  }) => Promise<void>,
): Promise<void> {
  return crawlerLoop(
    solana.getRpcHttp(),
    programAddress,
    initialCheckpoint,
    async ({ updatedCheckpoint, discoveredTransactions }) => {
      const startResolveTimeMs = Date.now();
      const discoveredInstructions = await indexerResolve(
        solana,
        programAddress,
        discoveredTransactions,
      );
      utilLogWithTimestamp(
        programAddress,
        `Resolved: Transactions x${discoveredTransactions.length}`,
        Date.now() - startResolveTimeMs,
      );
      const startProcessTimeMs = Date.now();
      await onProgress({ updatedCheckpoint, discoveredInstructions });
      utilLogWithTimestamp(
        programAddress,
        `Processed: Instructions x${discoveredInstructions.length}`,
        Date.now() - startProcessTimeMs,
      );
    },
  );
}
