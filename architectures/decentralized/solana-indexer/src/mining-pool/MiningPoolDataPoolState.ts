import {
  jsonCodecBoolean,
  jsonCodecInteger,
  jsonCodecNumber,
  jsonCodecObject,
  jsonCodecString,
} from "solana-kiss";

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

export const miningPoolDataPoolStateJsonCodec = jsonCodecObject({
  bump: jsonCodecNumber,
  index: jsonCodecInteger,
  authority: jsonCodecString,
  collateralMint: jsonCodecString,
  maxDepositCollateralAmount: jsonCodecInteger,
  totalDepositedCollateralAmount: jsonCodecInteger,
  totalExtractedCollateralAmount: jsonCodecInteger,
  claimingEnabled: jsonCodecBoolean,
  redeemableMint: jsonCodecString,
  totalClaimedRedeemableAmount: jsonCodecInteger,
  freeze: jsonCodecBoolean,
});
