import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  IndexingCheckpoint,
  indexingCheckpointJsonType,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { saveRead, saveWrite } from "../save";
import {
  CoordinatorDataStore,
  coordinatorDataStoreJsonType,
} from "./CoordinatorDataStore";
import { coordinatorIndexingCheckpoint } from "./CoordinatorIndexingCheckpoint";
import { coordinatorIndexingInstruction } from "./CoordinatorIndexingInstruction";

export async function coordinatorService(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
) {
  const saveName = `coordinator_${cluster}_${programAddress.toBase58()}.json`;
  const { checkpoint, dataStore } = await coordinatorServiceLoader(saveName);
  // TODO - add API calls here to serve data from dataStore
  await coordinatorServiceIndexing(
    saveName,
    endpoint,
    programAddress,
    checkpoint,
    dataStore,
  );
}

export async function coordinatorServiceLoader(saveName: string) {
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointJsonType.decode(saveContent.checkpoint);
    dataStore = coordinatorDataStoreJsonType.decode(saveContent.dataStore);
    console.log("Loaded coordinator state from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = { indexedChunks: [] };
    dataStore = new CoordinatorDataStore(new Map());
    console.warn("Failed to read existing coordinator JSON, starting fresh");
  }
  return { checkpoint, dataStore };
}

export async function coordinatorServiceIndexing(
  saveName: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: CoordinatorDataStore,
): Promise<void> {
  const idlService = new ToolboxIdlService();
  const idlProgram = await idlService.getOrResolveProgram(
    endpoint,
    programAddress,
  );
  if (idlProgram === undefined) {
    throw new Error(`Failed to resolve program IDL: ${programAddress}`);
  }
  await indexingInstructionsLoop(
    endpoint,
    programAddress,
    startingCheckpoint,
    idlProgram,
    async (
      instructionName,
      instructionAddresses,
      instructionPayload,
      ordering,
    ) => {
      await coordinatorIndexingInstruction(
        dataStore,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
      );
    },
    async (checkpoint) => {
      await coordinatorIndexingCheckpoint(dataStore, idlService, endpoint);
      await saveWrite(saveName, {
        checkpoint: indexingCheckpointJsonType.encode(checkpoint),
        dataStore: coordinatorDataStoreJsonType.encode(dataStore),
      });
    },
  );
}
