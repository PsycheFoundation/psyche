import {
  JsonCodec,
  jsonCodecArray,
  jsonCodecDateTime,
  jsonCodecInteger,
  jsonCodecObject,
  jsonCodecOptional,
  jsonCodecPubkey,
  jsonCodecRaw,
  jsonCodecString,
  JsonValue,
  Pubkey,
} from "solana-kiss";
import {
  utilsObjectToPubkeyMapJsonCodec,
  utilsObjectToStringMapJsonCodec,
} from "../utils";
import {
  MiningPoolDataPoolState,
  miningPoolDataPoolStateJsonCodec,
} from "./MiningPoolDataPoolState";

export interface MiningPoolDataPoolInfo {
  accountState: MiningPoolDataPoolState | undefined;
  accountUpdatedAt: Date | undefined;
  changeAcknowledgedOrdinal: bigint;
  changeNotificationOrdinal: bigint;
  depositCollateralAmountPerUser: Map<Pubkey, bigint>;
  claimRedeemableAmountPerUser: Map<Pubkey, bigint>;
  totalDepositCollateralAmount: bigint;
  totalClaimRedeemableAmount: bigint;
  totalExtractCollateralAmount: bigint;
  importantHistory: Array<{
    blockTime: Date | undefined;
    instructionOrdinal: bigint;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
  }>;
}

export const miningPoolDataPoolInfoJsonCodec: JsonCodec<MiningPoolDataPoolInfo> =
  jsonCodecObject({
    accountState: jsonCodecOptional(miningPoolDataPoolStateJsonCodec),
    accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
    changeAcknowledgedOrdinal: jsonCodecInteger,
    changeNotificationOrdinal: jsonCodecInteger,
    depositCollateralAmountPerUser:
      utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
    claimRedeemableAmountPerUser:
      utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
    totalDepositCollateralAmount: jsonCodecInteger,
    totalClaimRedeemableAmount: jsonCodecInteger,
    totalExtractCollateralAmount: jsonCodecInteger,
    importantHistory: jsonCodecArray(
      jsonCodecObject({
        blockTime: jsonCodecOptional(jsonCodecDateTime),
        instructionOrdinal: jsonCodecInteger,
        instructionName: jsonCodecString,
        instructionAddresses: utilsObjectToStringMapJsonCodec(jsonCodecPubkey),
        instructionPayload: jsonCodecRaw,
      }),
    ),
  });
