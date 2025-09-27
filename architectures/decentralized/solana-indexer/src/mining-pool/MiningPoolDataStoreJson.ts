import {
  jsonTypeBoolean,
  jsonTypeConst,
  jsonTypeNullableToOptional,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeString,
  jsonTypeStringToBigint,
  jsonTypeWrap,
} from "../jsonType";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export const miningPoolDataStorePoolAccountJsonTypeV1 = jsonTypeObject({
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
});

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

const temporary = jsonTypeWrap(
  miningPoolDataStoreJsonTypeV1,
  (decoded) => {
    return new MiningPoolDataStore(decoded.pools);
  },
  (encoded) => {
    return {
      version: 1 as const,
      pools: encoded.getPools(),
    };
  },
);

export const miningPoolDataStoreJsonType = temporary;
