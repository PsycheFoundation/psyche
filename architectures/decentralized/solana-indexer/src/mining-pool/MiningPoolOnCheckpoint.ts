import {
  MiningPoolAnalysis,
  miningPoolOnchainJsonCodec,
} from "psyche-indexer-codecs";
import { Pubkey, Solana } from "solana-kiss";
import { utilRunInParallel } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolOnCheckpoint(
  solana: Solana,
  dataStore: MiningPoolDataStore,
) {
  const tasks = await utilRunInParallel(
    dataStore.poolAnalysisByAddress.entries(),
    async ([poolAddress, poolAnalysis]) => {
      return await poolCheckpoint(solana, poolAddress, poolAnalysis);
    },
  );
  for (const task of tasks) {
    if (task.result.error) {
      console.error(
        "Failed to process pool checkpoint",
        task.input[0],
        task.result.error,
      );
    }
  }
}

async function poolCheckpoint(
  solana: Solana,
  poolAddress: Pubkey,
  poolAnalysis: MiningPoolAnalysis,
) {
  if (
    poolAnalysis.latestUpdateFetchOrdinal ===
    poolAnalysis.latestKnownChangeOrdinal
  ) {
    return;
  }
  poolAnalysis.latestUpdateFetchOrdinal = poolAnalysis.latestKnownChangeOrdinal;
  const { accountState } =
    await solana.getAndInferAndDecodeAccount(poolAddress);
  poolAnalysis.latestOnchainSnapshot = {
    native: accountState,
    parsed: miningPoolOnchainJsonCodec.decoder(accountState),
    updatedAt: new Date(),
  };
}
