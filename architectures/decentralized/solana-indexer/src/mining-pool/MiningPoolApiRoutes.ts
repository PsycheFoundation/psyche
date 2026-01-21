import { Application } from "express";
import {
  Pubkey,
  jsonCodecArrayToArray,
  jsonCodecBigInt,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  pubkeyFindPdaAddress,
  utf8Encode,
} from "solana-kiss";
import { MiningPoolDataStore } from "./MiningPoolDataStore";
import {
  miningPoolDataPoolAnalysisJsonCodec,
  miningPoolDataPoolOnchainJsonCodec,
} from "./MiningPoolDataTypes";

export async function miningPoolApiRoutes(
  programAddress: Pubkey,
  expressApplication: Application,
  dataStore: MiningPoolDataStore,
) {
  expressApplication.get(
    `${programAddress}/mining-pool/summaries`,
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
        .json(poolSummariesJsonCodec.encoder(poolsSummaries));
    },
  );
  expressApplication.get(
    `/${programAddress}/mining-pool/pool/:index`,
    (req, res) => {
      const poolIndex = jsonCodecBigInt.decoder(req.params.index);
      const poolAddress = getPoolAddress(programAddress, poolIndex);
      const poolAnalysis = dataStore.poolAnalysisByAddress.get(poolAddress);
      if (poolAnalysis === undefined) {
        return res.status(404).json({ error: "Pool not found" });
      }
      return res
        .status(200)
        .json(miningPoolDataPoolAnalysisJsonCodec.encoder(poolAnalysis));
    },
  );
}

function getPoolAddress(programAddress: Pubkey, poolIndex: bigint): Pubkey {
  const poolIndexSeed = new Uint8Array(8);
  new DataView(poolIndexSeed.buffer).setBigUint64(0, poolIndex, true);
  return pubkeyFindPdaAddress(programAddress, [
    utf8Encode("Pool"),
    poolIndexSeed,
  ]);
}

const poolSummariesJsonCodec = jsonCodecArrayToArray(
  jsonCodecObjectToObject({
    address: jsonCodecPubkey,
    state: miningPoolDataPoolOnchainJsonCodec,
  }),
);
