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
  coordinatorDataFromJson,
  coordinatorDataToJson,
} from "./CoordinatorDataJson";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const saveName = `coordinator_${cluster}_${programAddress.toBase58()}.json`;
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointFromJson(saveContent.checkpoint);
    dataStore = coordinatorDataFromJson(saveContent.dataStore);
    console.log("Loaded coordinator state saved from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new CoordinatorDataStore(new Map());
    console.warn("Failed to read existing coordinator JSON, starting fresh");
  }
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
    checkpoint,
    idlProgram,
    async (instructionName, instructionAddresses, instructionPayload) => {
      console.log("instructionName", instructionName);
      if (instructionName === "tick") {
        return await coordinatorProcessTick(
          dataStore,
          instructionAddresses,
          instructionPayload,
        );
      }
      if (instructionName === "witness") {
        return await coordinatorProcessWitness(
          dataStore,
          instructionAddresses,
          instructionPayload,
        );
      }
    },
    async (checkpoint) => {
      await saveWrite(saveName, {
        updatedAt: new Date().toISOString(),
        checkpoint: indexingCheckpointToJson(checkpoint),
        dataStore: coordinatorDataToJson(dataStore),
      });
    },
  );
}

export async function coordinatorProcessTick(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
): Promise<void> {}

export async function coordinatorProcessWitness(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
): Promise<void> {
  console.log("witness", instructionPayload.metadata);
  console.log(
    "eval",
    JSON.stringify(instructionPayload.metadata.evals, null, 2),
  );
}
