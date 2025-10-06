import { Pubkey, RpcHttp } from "solana-kiss";
import {
  IndexingCheckpoint,
  indexingCheckpointJsonType,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { saveRead, saveWrite } from "../save";
import { utilsGetProgramAnchorIdl } from "../utils";
import {
  CoordinatorDataStore,
  coordinatorDataStoreJsonType,
} from "./CoordinatorDataStore";
import { coordinatorIndexingCheckpoint } from "./CoordinatorIndexingCheckpoint";
import { coordinatorIndexingInstruction } from "./CoordinatorIndexingInstruction";

export async function coordinatorService(
  cluster: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
) {
  const saveName = `coordinator_${cluster}_${programAddress}`;
  const { checkpoint, dataStore } = await serviceLoader(saveName);
  // TODO - add API calls here to serve data from dataStore
  await serviceIndexing(
    saveName,
    rpcHttp,
    programAddress,
    checkpoint,
    dataStore,
  );
}

async function serviceLoader(saveName: string) {
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointJsonType.decoder(saveContent.checkpoint);
    dataStore = coordinatorDataStoreJsonType.decoder(saveContent.dataStore);
    console.log("Loaded coordinator state from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = { indexedChunks: [] };
    dataStore = new CoordinatorDataStore(new Map());
    console.warn(
      "Failed to read existing coordinator JSON, starting fresh",
      error,
    );
  }
  return { checkpoint, dataStore };
}

async function serviceIndexing(
  saveName: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: CoordinatorDataStore,
): Promise<void> {
  const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress);
  await indexingInstructionsLoop(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    programIdl,
    async (
      instructionName,
      instructionAddresses,
      instructionPayload,
      context,
    ) => {
      await coordinatorIndexingInstruction(
        dataStore,
        instructionName,
        instructionAddresses,
        instructionPayload,
        context.ordering,
        context.transaction.processedTime,
      );
    },
    async (checkpoint) => {
      await coordinatorIndexingCheckpoint(rpcHttp, programIdl, dataStore);
      await saveWrite(saveName, {
        checkpoint: indexingCheckpointJsonType.encoder(checkpoint),
        dataStore: coordinatorDataStoreJsonType.encoder(dataStore),
      });
    },
  );
}
