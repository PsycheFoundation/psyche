import {
  jsonTypeArrayToVariant,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
  jsonTypeStringToBigint,
} from "../json";
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

const jsonTypeV1 = jsonTypeArrayToVariant(
  "PoolInfo(v1)",
  jsonTypeObject({
    accountState: jsonTypeOptional(miningPoolDataPoolStateJsonType),
    accountFetchedOrdering: jsonTypeStringToBigint(),
    accountRequestOrdering: jsonTypeStringToBigint(),
    depositAmountPerUser: jsonTypeObjectToMap(jsonTypeStringToBigint()),
    depositAmountTotal: jsonTypeStringToBigint(),
  }),
);

export const miningPoolDataPoolInfoJsonType = jsonTypeV1;
