import {
  JsonType,
  jsonTypeArray,
  jsonTypeDate,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
  Pubkey,
} from "solana-kiss-data";
import { utilsOrderingJsonType } from "../utils";
import {
  MiningPoolDataPoolState,
  miningPoolDataPoolStateJsonType,
} from "./MiningPoolDataPoolState";

export interface MiningPoolDataPoolInfo {
  accountState: MiningPoolDataPoolState | undefined;
  accountUpdatedAt: Date | undefined;
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
  computedExtractedCollateralAmount: bigint;
  depositedCollateralAmountPerUser: Map<string, bigint>;
  computedDepositedCollateralAmount: bigint;
  claimedRedeemableAmountPerUser: Map<string, bigint>;
  computedClaimedRedeemableAmount: bigint;
  adminHistory: Array<{
    processedTime: Date | undefined;
    ordering: bigint;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
  }>;
}

export const miningPoolDataPoolInfoJsonType: JsonType<MiningPoolDataPoolInfo> =
  jsonTypeObject({
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountUpdatedAt: jsonTypeOptional(jsonTypeDate),
    accountFetchedOrdering: utilsOrderingJsonType,
    accountRequestOrdering: utilsOrderingJsonType,
    computedExtractedCollateralAmount: jsonTypeInteger,
    depositedCollateralAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    computedDepositedCollateralAmount: jsonTypeInteger,
    claimedRedeemableAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    computedClaimedRedeemableAmount: jsonTypeInteger,
    adminHistory: jsonTypeArray(
      jsonTypeObject({
        processedTime: jsonTypeOptional(jsonTypeDate),
        ordering: utilsOrderingJsonType,
        instructionName: jsonTypeString,
        instructionAddresses: jsonTypeObjectToMap(jsonTypeString),
        instructionPayload: jsonTypeValue,
      }),
    ),
  });
