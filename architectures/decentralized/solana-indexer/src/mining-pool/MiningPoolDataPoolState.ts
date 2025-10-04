import {
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
} from "solana-kiss-data";
import { utilsBigintStringJsonType } from "../utils";

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
  index: utilsBigintStringJsonType,
  authority: jsonTypeString,
  collateralMint: jsonTypeString,
  maxDepositCollateralAmount: utilsBigintStringJsonType,
  totalDepositedCollateralAmount: utilsBigintStringJsonType,
  totalExtractedCollateralAmount: utilsBigintStringJsonType,
  claimingEnabled: jsonTypeBoolean,
  redeemableMint: jsonTypeString,
  totalClaimedRedeemableAmount: utilsBigintStringJsonType,
  freeze: jsonTypeBoolean,
});
