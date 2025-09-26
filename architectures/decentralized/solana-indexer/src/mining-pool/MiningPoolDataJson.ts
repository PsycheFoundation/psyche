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

function camelToSnake(str: string): string {
  return str
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2") // insert underscore before capital letters
    .toLowerCase();
}

export const miningPoolDataStorePoolAccountJsonTypeV1 = jsonTypeObject(
  {
    bump: jsonTypeNumber(),
    index: jsonTypeStringToBigint(),
    authority: jsonTypeString(),
    collateralMint: jsonTypeString(),
    maxDepositCollateralAmount: jsonTypeStringToBigint(),
    totalDepositedCollateralAmount: jsonTypeStringToBigint(),
    totalExtractedCollateralAmount: jsonTypeStringToBigint(),
    claimingEnabled: jsonTypeBoolean(),
    redeemableMint: jsonTypeString(),
    totalClaimedRedeemableAmount: jsonTypeStringToBigint(),
    freeze: jsonTypeBoolean(),
  },
  camelToSnake,
);

export const miningPoolDataStoreJsonTypeV1 = jsonTypeObject({
  version: jsonTypeConst(1),
  pools: jsonTypeObjectToMap(
    jsonTypeObject({
      latestAccountState: jsonTypeNullableToOptional(
        miningPoolDataStorePoolAccountJsonTypeV1,
      ),
      latestAccountOrdering: jsonTypeStringToBigint(),
      depositAmountPerUser: jsonTypeObjectToMap(jsonTypeStringToBigint()),
    }),
  ),
});

export function miningPoolDataStoreToJson(
  dataStore: MiningPoolDataStore,
): JsonValue {
  return miningPoolDataStoreJsonTypeV1.encode({
    version: 1,
    pools: dataStore.getPools(),
  });
}

export function miningPoolDataStoreFromJson(
  jsonValue: JsonValue,
): MiningPoolDataStore {
  const decoded = miningPoolDataStoreJsonTypeV1.decode(jsonValue);
  return new MiningPoolDataStore(decoded.pools);
}
