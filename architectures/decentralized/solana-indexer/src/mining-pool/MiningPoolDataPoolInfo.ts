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
  totalExtractCollateralAmount: bigint;
  updates: Array<{
    ordering: bigint;
    payload: JsonValue;
  }>;
}

export const miningPoolDataPoolInfoJsonType: JsonType<MiningPoolDataPoolInfo> =
  jsonTypeObject({
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountFetchedOrdering: utilsOrderingJsonType,
    accountRequestOrdering: utilsOrderingJsonType,
    depositCollateralAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
    totalDepositCollateralAmount: jsonTypeInteger,
    totalExtractCollateralAmount: jsonTypeInteger,
    updates: jsonTypeArray(
      jsonTypeObject({
        ordering: utilsOrderingJsonType,
        payload: jsonTypeValue,
      }),
    ),
  });
