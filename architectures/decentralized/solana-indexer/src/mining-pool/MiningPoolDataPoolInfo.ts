import {
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
} from "solana-kiss-data";
import { utilsBigintStringJsonType } from "../utils";
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
  accountFetchedOrdering: utilsBigintStringJsonType,
  accountRequestOrdering: utilsBigintStringJsonType,
  depositCollateralAmountPerUser: jsonTypeObjectToMap(
    utilsBigintStringJsonType,
  ),
  totalDepositCollateralAmount: utilsBigintStringJsonType,
  totalExtractCollateralAmount: utilsBigintStringJsonType,
});
