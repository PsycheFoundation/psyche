import {
  JsonType,
  jsonTypeArray,
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
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
  depositCollateralAmountPerUser: Map<string, bigint>;
  totalDepositCollateralAmount: bigint;
  claimRedeemableAmountPerUser: Map<string, bigint>;
  totalClaimRedeemableAmount: bigint;
  updates: Array<{
    ordering: bigint;
    payload: JsonValue;
  }>;
  claimables: Array<{
    ordering: bigint;
    payload: JsonValue;
  }>;
  totalExtractCollateralAmount: bigint;
}

export const miningPoolDataPoolInfoJsonType: JsonType<MiningPoolDataPoolInfo> =
  jsonTypeObject({
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountFetchedOrdering: utilsOrderingJsonType,
    accountRequestOrdering: utilsOrderingJsonType,
    depositCollateralAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    totalDepositCollateralAmount: jsonTypeInteger,
    claimRedeemableAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    totalClaimRedeemableAmount: jsonTypeInteger,
    updates: jsonTypeArray(
      jsonTypeObject({
        ordering: utilsOrderingJsonType,
        payload: jsonTypeValue,
      }),
    ),
    claimables: jsonTypeArray(
      jsonTypeObject({
        ordering: utilsOrderingJsonType,
        payload: jsonTypeValue,
      }),
    ),
    totalExtractCollateralAmount: jsonTypeInteger,
  });
