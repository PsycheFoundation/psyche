import {
  JsonType,
  jsonTypeArray,
  jsonTypeDate,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
  jsonTypeValue,
  JsonValue,
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
  depositCollateralAmountPerUser: Map<string, bigint>;
  totalDepositCollateralAmount: bigint;
  claimRedeemableAmountPerUser: Map<string, bigint>;
  totalClaimRedeemableAmount: bigint;
  updates: Array<{
    processedTime: Date | undefined;
    ordering: bigint;
    payload: JsonValue;
  }>;
  claimables: Array<{
    processedTime: Date | undefined;
    ordering: bigint;
    payload: JsonValue;
  }>;
  totalExtractCollateralAmount: bigint;
}

export const miningPoolDataPoolInfoJsonType: JsonType<MiningPoolDataPoolInfo> =
  jsonTypeObject({
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountUpdatedAt: jsonTypeOptional(jsonTypeDate),
    accountFetchedOrdering: utilsOrderingJsonType,
    accountRequestOrdering: utilsOrderingJsonType,
    depositCollateralAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    totalDepositCollateralAmount: jsonTypeInteger,
    claimRedeemableAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    totalClaimRedeemableAmount: jsonTypeInteger,
    updates: jsonTypeArray(
      jsonTypeObject({
        processedTime: jsonTypeOptional(jsonTypeDate),
        ordering: utilsOrderingJsonType,
        payload: jsonTypeValue,
      }),
    ),
    claimables: jsonTypeArray(
      jsonTypeObject({
        processedTime: jsonTypeOptional(jsonTypeDate),
        ordering: utilsOrderingJsonType,
        payload: jsonTypeValue,
      }),
    ),
    totalExtractCollateralAmount: jsonTypeInteger,
  });
