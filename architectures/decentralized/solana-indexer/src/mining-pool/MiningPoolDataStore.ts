import {
  MiningPoolAnalysis,
  miningPoolAnalysisJsonCodec,
} from "psyche-indexer-codecs";
import {
  Pubkey,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecWrapped,
} from "solana-kiss";
import { jsonCodecObjectToMapByPubkey } from "../json";

export class MiningPoolDataStore {
  programAddress: Pubkey; // TODO - this might not belong here ??
  poolAnalysisByAddress: Map<Pubkey, MiningPoolAnalysis>;

  constructor(
    programAddress: Pubkey,
    poolAnalysisByAddress: Map<Pubkey, MiningPoolAnalysis>,
  ) {
    this.programAddress = programAddress;
    this.poolAnalysisByAddress = poolAnalysisByAddress;
  }

  public getPoolAnalysis(poolAddress: Pubkey): MiningPoolAnalysis {
    let poolAnalysis = this.poolAnalysisByAddress.get(poolAddress);
    if (poolAnalysis === undefined) {
      poolAnalysis = {
        latestKnownChangeOrdinal: 0n,
        latestUpdateFetchOrdinal: 0n,
        latestOnchainSnapshot: null,
        depositCollateralAmountPerUser: new Map<Pubkey, bigint>(),
        claimRedeemableAmountPerUser: new Map<Pubkey, bigint>(),
        totalExtractCollateralAmount: 0n,
        totalDepositCollateralAmount: 0n,
        totalClaimRedeemableAmount: 0n,
        adminHistory: [],
      };
      this.poolAnalysisByAddress.set(poolAddress, poolAnalysis);
    }
    return poolAnalysis;
  }
}

export const miningPoolDataStoreJsonCodec = jsonCodecWrapped(
  jsonCodecObjectToObject({
    programAddress: jsonCodecPubkey,
    poolAnalysisByAddress: jsonCodecObjectToMapByPubkey(
      miningPoolAnalysisJsonCodec,
    ),
  }),
  {
    decoder: (encoded) =>
      new MiningPoolDataStore(
        encoded.programAddress,
        encoded.poolAnalysisByAddress,
      ),
    encoder: (decoded) => ({
      programAddress: decoded.programAddress,
      poolAnalysisByAddress: decoded.poolAnalysisByAddress,
    }),
  },
);
