import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { JsonValue } from "../json";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingCheckpoint(
  dataStore: CoordinatorDataStore,
  idlService: ToolboxIdlService,
  endpoint: ToolboxEndpoint,
) {
  for (const runAddress of dataStore.getInvalidatedRunsAddresses()) {
    const accountInfo = await idlService.getAndInferAndDecodeAccount(
      endpoint,
      new PublicKey(runAddress),
    );
    coordinatorIndexingCheckpointRunAccountState(
      dataStore,
      runAddress,
      accountInfo.state as JsonValue,
    );
  }
}

export async function coordinatorIndexingCheckpointRunAccountState(
  dataStore: CoordinatorDataStore,
  runAddress: string,
  accountState: JsonValue,
): Promise<void> {
  console.log("Refreshing run account state", runAddress, accountState);
  try {
    dataStore.saveRunAccountState(runAddress, { runId: "hello world!" });
  } catch (error) {
    console.error("Failed to parse run account state", runAddress, error);
  }
}
