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
  depositAmountPerUser: Map<string, bigint>;
  depositAmountTotal: bigint;
}

export const miningPoolDataPoolInfoJsonType = jsonTypeObject({
  accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
  accountFetchedOrdering: utilsBigintStringJsonType,
  accountRequestOrdering: utilsBigintStringJsonType,
  depositAmountPerUser: jsonTypeObjectToMap(utilsBigintStringJsonType),
  depositAmountTotal: utilsBigintStringJsonType,
});
