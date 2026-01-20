import { Pubkey, RpcHttp } from "solana-kiss";
import { crawlerStep } from "./CrawlerStep";
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
      const { updatedCheckpoint, discoveredTransactions } = await crawlerStep(
        rpcHttp,
        programAddress,
        currentCheckpoint,
      );
      currentCheckpoint = updatedCheckpoint;
      await onProgress({ updatedCheckpoint, discoveredTransactions });
    } catch (error) {
      console.error("Crawling loop error, continuing", error);
    }
  }
}
