import {
  JsonType,
  jsonTypeNullableToOptional,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeObjectToVariant,
  jsonTypeStringToBigint,
  jsonTypeWithDecodeFallbacks,
} from "../jsonType";
import { miningPoolDataPoolAccountStateJsonType } from "./MiningPoolIndexingCheckpoint";

export interface MiningPoolDataPoolAccountState {
  bump: number;
  index: bigint;
  authority: string;
  collateralMint: string;
  maxDepositCollateralAmount: bigint;
  totalDepositedCollateralAmount: bigint;
  totalExtractedCollateralAmount: bigint;
  claimingEnabled: boolean;
  redeemableMint: string;
  totalClaimedRedeemableAmount: bigint;
  freeze: boolean;
}

export interface MiningPoolDataPoolDetails {
  latestAccountState: MiningPoolDataPoolAccountState | undefined;
  latestAccountOrdering: bigint;
  depositAmountPerUser: Map<string, bigint>;
}

const jsonTypeV1 = jsonTypeObjectToVariant(
  "pool_v1",
  jsonTypeObject({
    latestAccountState: jsonTypeNullableToOptional(
      miningPoolDataPoolAccountStateJsonType,
    ),
    latestAccountOrdering: jsonTypeStringToBigint(),
    depositAmountPerUser: jsonTypeObjectToMap(jsonTypeStringToBigint()),
  }),
);

export const miningPoolDataPoolDetailsJsonType: JsonType<MiningPoolDataPoolDetails> =
  jsonTypeWithDecodeFallbacks(jsonTypeV1, []);
