import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
  JsonValue,
} from "../json";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export const miningPoolDataPoolAccountStateJsonType = jsonTypeObject({
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

export async function miningPoolIndexingCheckpoint(
  dataStore: MiningPoolDataStore,
  idlService: ToolboxIdlService,
  endpoint: ToolboxEndpoint,
) {
  for (const poolAddress of dataStore.getInvalidatedPoolsAddresses()) {
    try {
      const poolAccountInfo = await idlService.getAndInferAndDecodeAccount(
        endpoint,
        new PublicKey(poolAddress),
      );
      const accountState = poolAccountInfo.state as JsonValue;
      console.log("Refreshing pool account state", poolAddress, accountState);
      dataStore.savePoolAccountState(
        poolAddress,
        miningPoolDataPoolAccountStateJsonType.decode(accountState),
      );
    } catch (error) {
      console.error("Failed to refresh pool account state", poolAddress, error);
    }
  }
}
