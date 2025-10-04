import {
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeString,
} from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import {
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
  const promises = new Array<Promise<void>>();
  for (const [runAddress, runInfo] of dataStore.runsInfos) {
    if (runInfo.accountFetchedOrdering === runInfo.accountRequestOrdering) {
      break;
    }
    const promise = updateCoordinatorAccountState(
      rpcHttp,
      programIdl,
      dataStore,
      runAddress,
    );
    promises.push(promise);
  }
  await Promise.all(promises);
}

async function updateCoordinatorAccountState(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  dataStore: CoordinatorDataStore,
  runAddress: string,
) {
  try {
    const runState = runStateJsonDecoder(
      await utilsGetAndDecodeAccountState(rpcHttp, programIdl, runAddress),
    );
    // console.log("Refreshed run state", runAddress, runState.nonce);
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

const runStateJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  nonce: jsonTypeInteger.decoder,
  state: utilsObjectSnakeCaseJsonDecoder({
    metadata: utilsObjectSnakeCaseJsonDecoder({
      name: utilsRustFixedStringJsonDecoder,
      description: utilsRustFixedStringJsonDecoder,
      numParameters: jsonTypeInteger.decoder,
      vocabSize: jsonTypeInteger.decoder,
    }),
    coordinator: utilsObjectSnakeCaseJsonDecoder({
      runId: utilsRustFixedStringJsonDecoder,
      runState: jsonTypeString.decoder,
      progress: utilsObjectSnakeCaseJsonDecoder({
        epoch: jsonTypeNumber.decoder,
        step: jsonTypeNumber.decoder,
        epochStartDataIndex: jsonTypeInteger.decoder,
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
