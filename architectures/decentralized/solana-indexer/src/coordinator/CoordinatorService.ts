import { Pubkey } from "solana-kiss-data";
import { resolveProgramAnchorIdl } from "solana-kiss-resolve";
import { RpcHttp } from "solana-kiss-rpc";
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
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
) {
  const saveName = `coordinator_${cluster}_${programAddress}`;
  const { checkpoint, dataStore } = await coordinatorServiceLoader(saveName);
  // TODO - add API calls here to serve data from dataStore
  await coordinatorServiceIndexing(
    saveName,
    rpcHttp,
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
    checkpoint = indexingCheckpointJsonType.decoder(saveContent.checkpoint);
    dataStore = coordinatorDataStoreJsonType.decoder(saveContent.dataStore);
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
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: CoordinatorDataStore,
): Promise<void> {
  const programIdl = await resolveProgramAnchorIdl(rpcHttp, programAddress);
  if (programIdl === undefined) {
    throw new Error(`Failed to resolve program IDL: ${programAddress}`);
  }
  await indexingInstructionsLoop(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    programIdl,
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
      await coordinatorIndexingCheckpoint(rpcHttp, programIdl, dataStore);
      await saveWrite(saveName, {
        checkpoint: indexingCheckpointJsonType.encoder(checkpoint),
        dataStore: coordinatorDataStoreJsonType.encoder(dataStore),
      });
    },
  );
}
