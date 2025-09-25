import { JsonValue } from "../json";
import {
  jsonTypeBoolean,
  jsonTypeNullableToOptional,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../jsonType";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

const jsonTypePool = jsonTypeObject({
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
  version: jsonTypeNumber(),
  pools: jsonTypeObjectToMap(
    jsonTypeObject({
      latestAccountState: jsonTypeNullableToOptional(jsonTypePool),
      latestAccountOrdering: jsonTypeStringToBigint(),
      depositAmountPerUser: jsonTypeObjectToMap(jsonTypeStringToBigint()),
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
  if (decoded.version !== 2) {
    throw new Error(`Unsupported mining pool data version`);
  }
  return new MiningPoolDataStore(decoded.pools);
}
