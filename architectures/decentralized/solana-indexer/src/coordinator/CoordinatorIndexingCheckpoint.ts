import { jsonTypeNumber, jsonTypeString } from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import {
  utilsBigintStringJsonType,
  utilsGetAndDecodeAccountState,
  utilsObjectSnakeCaseJsonDecoder,
  utilsRustFixedArrayJsonDecoder,
  utilsRustFixedStringJsonDecoder,
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
      const runState = runStateJsonDecoder(
        await utilsGetAndDecodeAccountState(rpcHttp, programIdl, runAddress),
      );
      console.log("Refreshed run state", runAddress, runState.nonce);
      dataStore.saveRunState(runAddress, {
        runId: runState.state.coordinator.runId,
        name: runState.state.metadata.name,
        description: runState.state.metadata.description,
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

const runStateJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  nonce: utilsBigintStringJsonType.decoder,
  state: utilsObjectSnakeCaseJsonDecoder({
    metadata: utilsObjectSnakeCaseJsonDecoder({
      name: utilsRustFixedStringJsonDecoder,
      description: utilsRustFixedStringJsonDecoder,
      numParameters: utilsBigintStringJsonType.decoder,
      vocabSize: utilsBigintStringJsonType.decoder,
    }),
    coordinator: utilsObjectSnakeCaseJsonDecoder({
      runId: utilsRustFixedStringJsonDecoder,
      runState: jsonTypeString.decoder,
      progress: utilsObjectSnakeCaseJsonDecoder({
        epoch: jsonTypeNumber.decoder,
        step: jsonTypeNumber.decoder,
        epochStartDataIndex: utilsBigintStringJsonType.decoder,
      }),
      epochState: utilsObjectSnakeCaseJsonDecoder({
        clients: utilsRustFixedArrayJsonDecoder(
          utilsObjectSnakeCaseJsonDecoder({
            id: utilsObjectSnakeCaseJsonDecoder({
              signer: jsonTypeString.decoder,
            }),
            state: jsonTypeString.decoder,
          }),
        ),
      }),
    }),
  }),
});
