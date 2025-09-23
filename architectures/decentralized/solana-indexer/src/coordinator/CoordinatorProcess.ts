import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import {
  IndexingCheckpoint,
  indexingCheckpointFromJson,
  indexingCheckpointToJson,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { jsonSchemaObject, jsonSchemaString, jsonSchemaValue } from "../json";
import {
  coordinatorDataFromJson,
  coordinatorDataToJson,
} from "./CoordinatorDataJson";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

const saveFileJsonSchema = jsonSchemaObject({
  updatedAt: jsonSchemaString(),
  checkpoint: jsonSchemaValue(),
  dataStore: jsonSchemaValue(),
});

export async function coordinatorProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const fileJson = `coordinator_${cluster}_${programAddress.toBase58()}.json`;
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const jsonValue = await fileJsonRead(fileJson);
    const jsonContent = saveFileJsonSchema.parse(jsonValue);
    checkpoint = indexingCheckpointFromJson(jsonContent.checkpoint);
    dataStore = coordinatorDataFromJson(jsonContent.dataStore);
    console.log(
      "Loaded coordinator state from JSON from:",
      jsonContent.updatedAt,
    );
  } catch (error) {
    console.warn("Failed to read existing coordinator JSON, starting fresh");
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new CoordinatorDataStore();
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
      await fileJsonWrite(fileJson, {
        updatedAt: new Date().toISOString(),
        checkpoint: indexingCheckpointToJson(checkpoint),
        store: coordinatorDataToJson(dataStore),
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
