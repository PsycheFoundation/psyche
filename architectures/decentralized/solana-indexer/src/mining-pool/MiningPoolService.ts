import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  IndexingCheckpoint,
  indexingCheckpointFromJson,
  indexingCheckpointToJson,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { saveRead, saveWrite } from "../save";
import {
  miningPoolDataStoreFromJson,
  miningPoolDataStoreToJson,
} from "./MiningPoolDataJson";
import { MiningPoolDataStore } from "./MiningPoolDataStore";
import { miningPoolIndexingCheckpoint } from "./MiningPoolIndexingCheckpoint";
import { miningPoolIndexingInstruction } from "./MiningPoolIndexingInstruction";

export async function miningPoolService(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const saveName = `mining_pool_${cluster}_${programAddress.toBase58()}`;
  const { checkpoint, dataStore } = await miningPoolServiceLoader(saveName);
  await miningPoolServiceIndexing(
    saveName,
    endpoint,
    programAddress,
    checkpoint,
    dataStore,
  );
}

export async function miningPoolServiceLoader(saveName: string) {
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointFromJson(saveContent.checkpoint);
    dataStore = miningPoolDataStoreFromJson(saveContent.dataStore);
    console.log("Loaded mining pool state saved from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new MiningPoolDataStore(new Map());
    console.warn("Failed to read existing mining pool JSON, starting fresh");
  }
  return { checkpoint, dataStore };
}

export async function miningPoolServiceIndexing(
  saveName: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: MiningPoolDataStore,
) {
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
      await miningPoolIndexingInstruction(
        dataStore,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
      );
    },
    async (checkpoint) => {
      await miningPoolIndexingCheckpoint(dataStore, idlService, endpoint);
      await saveWrite(saveName, {
        updatedAt: new Date().toISOString(),
        checkpoint: indexingCheckpointToJson(checkpoint),
        dataStore: miningPoolDataStoreToJson(dataStore),
      });
    },
  );
}
