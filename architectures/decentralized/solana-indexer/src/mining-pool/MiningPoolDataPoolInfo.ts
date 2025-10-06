import {
  JsonType,
  jsonTypeArray,
  jsonTypeDateTime,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypeOptional,
  jsonTypePubkey,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
  Pubkey,
} from "solana-kiss";
import {
  utilsObjectToPubkeyMapJsonType,
  utilsObjectToStringMapJsonType,
} from "../utils";
import {
  MiningPoolDataPoolState,
  miningPoolDataPoolStateJsonType,
} from "./MiningPoolDataPoolState";

export interface MiningPoolDataPoolInfo {
  accountState: MiningPoolDataPoolState | undefined;
  accountUpdatedAt: Date | undefined;
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
  totalExtractCollateralAmount: bigint;
  depositCollateralAmountPerUser: Map<Pubkey, bigint>;
  totalDepositCollateralAmount: bigint;
  claimRedeemableAmountPerUser: Map<Pubkey, bigint>;
  totalClaimRedeemableAmount: bigint;
  adminHistory: Array<{
    signerAddress: Pubkey;
    processedTime: Date | undefined;
    ordering: bigint;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
  }>;
}

export const miningPoolDataPoolInfoJsonType: JsonType<MiningPoolDataPoolInfo> =
  jsonTypeObject((key) => key, {
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountUpdatedAt: jsonTypeOptional(jsonTypeDateTime),
    accountFetchedOrdering: jsonTypeInteger,
    accountRequestOrdering: jsonTypeInteger,
    totalExtractCollateralAmount: jsonTypeInteger,
    depositCollateralAmountPerUser:
      utilsObjectToPubkeyMapJsonType(jsonTypeInteger),
    totalDepositCollateralAmount: jsonTypeInteger,
    claimRedeemableAmountPerUser:
      utilsObjectToPubkeyMapJsonType(jsonTypeInteger),
    totalClaimRedeemableAmount: jsonTypeInteger,
    adminHistory: jsonTypeArray(
      jsonTypeObject((key) => key, {
        signerAddress: jsonTypePubkey,
        processedTime: jsonTypeOptional(jsonTypeDateTime),
        ordering: jsonTypeInteger,
        instructionName: jsonTypeString,
        instructionAddresses: utilsObjectToStringMapJsonType(jsonTypePubkey),
        instructionPayload: jsonTypeValue,
      }),
    ),
  });
