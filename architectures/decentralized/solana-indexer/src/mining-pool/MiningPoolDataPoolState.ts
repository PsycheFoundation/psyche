import {
  jsonTypeArrayToVariant,
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../json";

export interface MiningPoolDataPoolState {
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

const jsonTypeV1 = jsonTypeArrayToVariant(
  "PoolState(v1)",
  jsonTypeObject({
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
  }),
);

export const miningPoolDataPoolStateJsonType = jsonTypeV1;
