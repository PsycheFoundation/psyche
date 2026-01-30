import { Application } from "express";
import {
  miningPoolAddressFromIndex,
  miningPoolAnalysisJsonCodec,
  miningPoolSummaryJsonCodec,
} from "psyche-indexer-codecs";
import { Pubkey, jsonCodecBigInt, jsonEncoderArrayToArray } from "solana-kiss";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolApiRoutes(
  programAddress: Pubkey,
  expressApplication: Application,
  dataStore: MiningPoolDataStore,
) {
  expressApplication.get(
    `/${programAddress}/mining-pool/summaries`,
    (_, res) => {
      const poolsSummaries = [];
      for (const [
        poolAddress,
        poolAnalysis,
      ] of dataStore.poolAnalysisByAddress) {
        const poolState = poolAnalysis.latestOnchainSnapshot?.parsed;
        if (poolState === undefined) {
          continue;
        }
        poolsSummaries.push({ address: poolAddress, state: poolState });
      }
      return res
        .status(200)
        .json(
          jsonEncoderArrayToArray(miningPoolSummaryJsonCodec.encoder)(
            poolsSummaries,
          ),
        );
    },
  );
  expressApplication.get(
    `/${programAddress}/mining-pool/pool/:index`,
    (req, res) => {
      const poolIndex = jsonCodecBigInt.decoder(req.params.index);
      const poolAddress = miningPoolAddressFromIndex(programAddress, poolIndex);
      const poolAnalysis = dataStore.poolAnalysisByAddress.get(poolAddress);
      if (poolAnalysis === undefined) {
        return res.status(404).json({ error: "Pool not found" });
      }
      return res
        .status(200)
        .json(miningPoolAnalysisJsonCodec.encoder(poolAnalysis));
    },
  );
}
