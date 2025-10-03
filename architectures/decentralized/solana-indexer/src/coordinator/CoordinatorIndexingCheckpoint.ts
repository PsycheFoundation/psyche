import { jsonTypeNumber, jsonTypeString } from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import {
  getAndDecodeAccountState,
  jsonTypeObjectSnakeCase,
  jsonTypeRustFixedArray,
  jsonTypeRustFixedString,
  jsonTypeStringToBigint,
} from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingCheckpoint(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  dataStore: CoordinatorDataStore,
) {
  for (const [runAddress, runInfo] of dataStore.runsInfos) {
    if (runInfo.accountFetchedOrdering === runInfo.accountRequestOrdering) {
      break;
    }
    try {
      const runState = runStateJsonType.decode(
        await getAndDecodeAccountState(rpcHttp, programIdl, runAddress),
      );
      console.log("Refreshed run state", runAddress, runState.nonce);
      dataStore.saveRunState(runAddress, {
        runId: runState.runId,
        name: runState.name,
        description: runState.description,
        status: runState.status,
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
      runState: jsonTypeString,
      progress: jsonTypeObjectSnakeCase({
        epoch: jsonTypeNumber,
        step: jsonTypeNumber,
        epochStartDataIndex: jsonTypeStringToBigint(),
      }),
      epochState: jsonTypeObjectSnakeCase({
        clients: jsonTypeRustFixedArray(
          jsonTypeObjectSnakeCase({
            id: jsonTypeObjectSnakeCase({
              signer: jsonTypeString,
            }),
            state: jsonTypeString,
          }),
        ),
      }),
    }),
  }),
});
