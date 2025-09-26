import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { JsonValue } from "../json";
import { miningPoolDataStorePoolAccountJsonTypeV1 } from "./MiningPoolDataJson";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingCheckpoint(
  dataStore: MiningPoolDataStore,
  idlService: ToolboxIdlService,
  endpoint: ToolboxEndpoint,
) {
  for (const poolAddress of dataStore.getInvalidatedPoolsAddresses()) {
    const accountInfo = await idlService.getAndInferAndDecodeAccount(
      endpoint,
      new PublicKey(poolAddress),
    );
    processRefreshPoolAccountState(
      dataStore,
      poolAddress,
      accountInfo.state as JsonValue,
    );
  }
}

export async function processRefreshPoolAccountState(
  dataStore: MiningPoolDataStore,
  poolAddress: string,
  accountState: JsonValue,
): Promise<void> {
  console.log("Refreshing pool account state", poolAddress, accountState);
  try {
    dataStore.savePoolAccountState(
      poolAddress,
      miningPoolDataStorePoolAccountJsonTypeV1.decode(accountState),
    );
  } catch (error) {
    console.error("Failed to parse pool account state", poolAddress, error);
  }
}
