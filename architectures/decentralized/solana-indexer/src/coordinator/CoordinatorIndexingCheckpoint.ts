import {
  casingCamelToSnake,
  IdlProgram,
  jsonDecoderObject,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypePubkey,
  jsonTypeString,
  Pubkey,
  RpcHttp,
} from "solana-kiss";
import {
  utilsGetAndDecodeAccountState,
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
  runAddress: Pubkey,
) {
  try {
    const runState = await utilsGetAndDecodeAccountState(
      rpcHttp,
      programIdl,
      runAddress,
      runStateJsonDecoder,
    );
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

const runStateJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
  nonce: jsonTypeInteger.decoder,
  state: jsonDecoderObject(casingCamelToSnake, {
    metadata: jsonDecoderObject(casingCamelToSnake, {
      name: utilsRustFixedStringJsonDecoder,
      description: utilsRustFixedStringJsonDecoder,
      numParameters: jsonTypeInteger.decoder,
      vocabSize: jsonTypeInteger.decoder,
    }),
    coordinator: jsonDecoderObject(casingCamelToSnake, {
      runId: utilsRustFixedStringJsonDecoder,
      runState: jsonTypeString.decoder,
      progress: jsonDecoderObject(casingCamelToSnake, {
        epoch: jsonTypeNumber.decoder,
        step: jsonTypeNumber.decoder,
        epochStartDataIndex: jsonTypeInteger.decoder,
      }),
      epochState: jsonDecoderObject(casingCamelToSnake, {
        clients: utilsRustFixedArrayJsonDecoder(
          jsonDecoderObject(casingCamelToSnake, {
            id: jsonDecoderObject(casingCamelToSnake, {
              signer: jsonTypePubkey.decoder,
            }),
            state: jsonTypeString.decoder,
          }),
        ),
      }),
    }),
  }),
});
