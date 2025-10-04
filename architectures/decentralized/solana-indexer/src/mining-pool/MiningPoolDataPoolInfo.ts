import {
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
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
}

export const miningPoolDataPoolInfoJsonType = jsonTypeObject({
  accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
  accountFetchedOrdering: utilsOrderingJsonType,
  accountRequestOrdering: utilsOrderingJsonType,
  depositCollateralAmountPerUser: jsonTypeObjectToMap(jsonTypeInteger),
  totalDepositCollateralAmount: jsonTypeInteger,
  totalExtractCollateralAmount: jsonTypeInteger,
});
