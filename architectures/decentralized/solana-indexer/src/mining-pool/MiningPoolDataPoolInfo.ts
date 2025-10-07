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
    processedTime: Date | undefined;
    signerAddress: Pubkey;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
    ordering: bigint;
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
        processedTime: jsonTypeOptional(jsonTypeDateTime),
        signerAddress: jsonTypePubkey,
        instructionName: jsonTypeString,
        instructionAddresses: utilsObjectToStringMapJsonType(jsonTypePubkey),
        instructionPayload: jsonTypeValue,
        ordering: jsonTypeInteger,
      }),
    ),
  });
