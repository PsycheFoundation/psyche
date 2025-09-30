import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  jsonTypeNumber,
  jsonTypeString,
  jsonTypeStringToBigint,
  JsonValue,
} from "../json";
import {
  jsonTypeObjectSnakeCase,
  jsonTypeRustFixedArray,
  jsonTypeRustFixedString,
} from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingCheckpoint(
  dataStore: CoordinatorDataStore,
  idlService: ToolboxIdlService,
  endpoint: ToolboxEndpoint,
) {
  for (const [runAddress, runInfo] of dataStore.runsInfos) {
    if (runInfo.accountFetchedOrdering === runInfo.accountRequestOrdering) {
      break;
    }
    try {
      const runAccount = await idlService.getAndInferAndDecodeAccount(
        endpoint,
        new PublicKey(runAddress),
      );
      const runState = runStateJsonType.decode(runAccount.state as JsonValue);
      console.log("Refreshed run state", runAddress, runState.nonce);
      dataStore.saveRunState(runAddress, {
        runId: runState.state.coordinator.runId.value,
        name: runState.state.metadata.name.value,
        description: runState.state.metadata.description.value,
        status: runState.state.coordinator.runState,
        epochClients: runState.state.coordinator.epochState.clients.map(
          (client) => ({
            signer: client.id.signer,
            state: client.state,
          }),
        ),
        nonce: runState.nonce,
      });
    } catch (error) {
      console.error("Failed to refresh run state", runAddress, error);
    }
  }
}

const runStateJsonType = jsonTypeObjectSnakeCase({
  nonce: jsonTypeStringToBigint(),
  state: jsonTypeObjectSnakeCase({
    metadata: jsonTypeObjectSnakeCase({
      name: jsonTypeRustFixedString(),
      description: jsonTypeRustFixedString(),
      numParameters: jsonTypeStringToBigint(),
      vocabSize: jsonTypeStringToBigint(),
    }),
    coordinator: jsonTypeObjectSnakeCase({
      runId: jsonTypeRustFixedString(),
      runState: jsonTypeString(),
      progress: jsonTypeObjectSnakeCase({
        epoch: jsonTypeNumber(),
        step: jsonTypeNumber(),
        epochStartDataIndex: jsonTypeStringToBigint(),
      }),
      epochState: jsonTypeObjectSnakeCase({
        clients: jsonTypeRustFixedArray(
          jsonTypeObjectSnakeCase({
            id: jsonTypeObjectSnakeCase({
              signer: jsonTypeString(),
            }),
            state: jsonTypeString(),
          }),
        ),
      }),
    }),
  }),
});
