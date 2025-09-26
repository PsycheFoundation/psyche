import { JsonValue } from "../json";
import {
  jsonTypeBoolean,
  jsonTypeConst,
  jsonTypeNullableToOptional,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../jsonType";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export const miningPoolJsonTypePool = jsonTypeObject({
  bump: jsonTypeNumber(),
  index: jsonTypeStringToBigint(),
  authority: jsonTypeString(),
  collateral_mint: jsonTypeString(),
  max_deposit_collateral_amount: jsonTypeStringToBigint(),
  total_deposited_collateral_amount: jsonTypeStringToBigint(),
  total_extracted_collateral_amount: jsonTypeStringToBigint(),
  claiming_enabled: jsonTypeBoolean(),
  redeemable_mint: jsonTypeString(),
  total_claimed_redeemable_amount: jsonTypeStringToBigint(),
  freeze: jsonTypeBoolean(),
});

const jsonTypeV2 = jsonTypeObject({
  version: jsonTypeConst(2),
  pools: jsonTypeObjectToMap(
    jsonTypeObject({
      latestAccountState: jsonTypeNullableToOptional(miningPoolJsonTypePool),
      latestAccountOrdering: jsonTypeStringToBigint(),
      depositAmountPerUser: jsonTypeObjectToMap(jsonTypeStringToBigint()),
      computedTotal1: jsonTypeStringToBigint(),
      computedTotal2: jsonTypeStringToBigint(),
    }),
  ),
});

/*
const jsonTypeV3 = jsonTypeObject({
  version: jsonTypeNumber(3),
  pools: jsonTypeObjectToMap(
    jsonTypeObject({
      latestAccountState: jsonTypeUnion(jsonTypeNull(), jsonTypeString()),
      latestAccountOrdering: jsonTypeString(),
      depositAmountPerUser: jsonTypeObjectToMap(jsonTypeString()),
    }),
  ),
});
*/

export function miningPoolDataToJson(
  dataStore: MiningPoolDataStore,
): JsonValue {
  return jsonTypeV2.encode({
    version: 2,
    pools: dataStore.getPools(),
  });
}

export function miningPoolDataFromJson(
  jsonValue: JsonValue,
): MiningPoolDataStore {
  const decoded = jsonTypeV2.decode(jsonValue);
  return new MiningPoolDataStore(decoded.pools);
}
