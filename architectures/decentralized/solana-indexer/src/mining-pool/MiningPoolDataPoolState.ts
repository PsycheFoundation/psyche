import {
  jsonTypeBoolean,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
} from "solana-kiss-data";

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

export const miningPoolDataPoolStateJsonType = jsonTypeObject({
  bump: jsonTypeNumber,
  index: jsonTypeInteger,
  authority: jsonTypeString,
  collateralMint: jsonTypeString,
  maxDepositCollateralAmount: jsonTypeInteger,
  totalDepositedCollateralAmount: jsonTypeInteger,
  totalExtractedCollateralAmount: jsonTypeInteger,
  claimingEnabled: jsonTypeBoolean,
  redeemableMint: jsonTypeString,
  totalClaimedRedeemableAmount: jsonTypeInteger,
  freeze: jsonTypeBoolean,
});
