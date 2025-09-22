import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import { IndexingCheckpoint } from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

function coordinatorJsonPath(
  cluster: string,
  programAddress: PublicKey,
): string {
  return `./coordinator_${cluster}_${programAddress.toBase58()}.json`;
}

export async function coordinatorProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const jsonPath = coordinatorJsonPath(cluster, programAddress);
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const snapshotJson = await fileJsonRead(jsonPath);
    checkpoint = IndexingCheckpoint.fromJson(snapshotJson.checkpoint);
    dataStore = CoordinatorDataStore.fromJson(snapshotJson.dataStore);
  } catch (error) {
    console.warn("Failed to read existing coordinator JSON, starting fresh");
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new CoordinatorDataStore();
  }
  await indexingInstructionsLoop(
    endpoint,
    programAddress,
    checkpoint,
    async (instructionName, instructionAddresses, instructionPayload) => {
      if (instructionName === "witness") {
        await coordinatorProcessWitness(
          dataStore,
          instructionAddresses,
          instructionPayload,
        );
      }
    },
    async (checkpoint) => {
      await fileJsonWrite(jsonPath, {
        updatedAt: new Date().toISOString(),
        checkpoint: checkpoint.toJson(),
        store: dataStore.toJson(),
      });
    },
  );
}

export async function coordinatorProcessWitness(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
): Promise<void> {
  //console.log("Processing witness instruction...");
  //console.log("instructionAddresses", instructionAddresses);
  //console.log("instructionPayload", instructionPayload);
}
