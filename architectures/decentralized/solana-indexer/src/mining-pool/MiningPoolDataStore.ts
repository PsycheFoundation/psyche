import {
  Pubkey,
  jsonCodecObject,
  jsonCodecPubkey,
  jsonCodecTransform,
  pubkeyFindPdaAddress,
  utf8Encode,
} from "solana-kiss";
import { utilsObjectToPubkeyMapJsonCodec } from "../utils";
import {
  MiningPoolDataPoolInfo,
  miningPoolDataPoolInfoJsonCodec,
} from "./MiningPoolDataPoolInfo";

export class MiningPoolDataStore {
  programAddress: Pubkey;
  poolInfoByAddress: Map<Pubkey, MiningPoolDataPoolInfo>;

  constructor(
    programAddress: Pubkey,
    poolInfoByAddress: Map<Pubkey, MiningPoolDataPoolInfo>,
  ) {
    this.programAddress = programAddress;
    this.poolInfoByAddress = poolInfoByAddress;
  }

  public getPoolAddress(poolIndex: bigint): Pubkey {
    const poolIndexSeed = new Uint8Array(8);
    new DataView(poolIndexSeed.buffer).setBigUint64(0, poolIndex, true);
    return pubkeyFindPdaAddress(this.programAddress, [
      utf8Encode("Pool"),
      poolIndexSeed,
    ]);
  }

  public getPoolInfo(poolAddress: Pubkey): MiningPoolDataPoolInfo {
    let poolInfo = this.poolInfoByAddress.get(poolAddress);
    if (poolInfo === undefined) {
      poolInfo = {
        accountState: undefined,
        accountUpdatedAt: undefined,
        changeAcknowledgedOrdinal: 0n,
        changeNotificationOrdinal: 0n,
        depositCollateralAmountPerUser: new Map<Pubkey, bigint>(),
        claimRedeemableAmountPerUser: new Map<Pubkey, bigint>(),
        totalExtractCollateralAmount: 0n,
        totalDepositCollateralAmount: 0n,
        totalClaimRedeemableAmount: 0n,
        importantHistory: [],
      };
      this.poolInfoByAddress.set(poolAddress, poolInfo);
    }
    return poolInfo;
  }
}

export const miningPoolDataStoreJsonCodec = jsonCodecTransform(
  jsonCodecObject({
    programAddress: jsonCodecPubkey,
    poolInfoByAddress: utilsObjectToPubkeyMapJsonCodec(
      miningPoolDataPoolInfoJsonCodec,
    ),
  }),
  {
    decoder: (encoded) =>
      new MiningPoolDataStore(
        encoded.programAddress,
        encoded.poolInfoByAddress,
      ),
    encoder: (decoded) => ({
      programAddress: decoded.programAddress,
      poolInfoByAddress: decoded.poolInfoByAddress,
    }),
  },
);
