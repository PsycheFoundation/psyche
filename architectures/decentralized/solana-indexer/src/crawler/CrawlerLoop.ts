import { Pubkey, RpcHttp } from "solana-kiss";
import { crawlerCycle } from "./CrawlerCycle";
import { CrawlerCheckpoint, CrawlerTransaction } from "./CrawlerTypes";

export async function crawlerLoop(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  initialCheckpoint: CrawlerCheckpoint,
  onProgress: (params: {
    updatedCheckpoint: CrawlerCheckpoint;
    discoveredTransactions: Array<CrawlerTransaction>;
  }) => Promise<void>,
): Promise<never> {
  let currentCheckpoint = initialCheckpoint;
  while (true) {
    try {
      const { updatedCheckpoint, discoveredTransactions } = await crawlerCycle(
        rpcHttp,
        programAddress,
        currentCheckpoint,
      );
      await onProgress({ updatedCheckpoint, discoveredTransactions });
      currentCheckpoint = updatedCheckpoint;
    } catch (error) {
      console.error("Crawling loop error, retrying", error);
    }
  }
}
