import { Pubkey, Solana } from "solana-kiss";
import { utilRunInParallel } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";
import {
  MiningPoolDataPoolAnalysis,
  miningPoolDataPoolOnchainJsonCodec,
} from "./MiningPoolDataTypes";

export async function miningPoolOnCheckpoint(
  solana: Solana,
  miningPoolDataStore: MiningPoolDataStore,
) {
  const tasks = await utilRunInParallel(
    miningPoolDataStore.poolAnalysisByAddress.entries(),
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
  poolAnalysis: MiningPoolDataPoolAnalysis,
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
    parsed: miningPoolDataPoolOnchainJsonCodec.decoder(accountState),
    updatedAt: new Date(),
  };
}
