import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import { IndexingCheckpoint } from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const fileJson = `coordinator_${cluster}_${programAddress.toBase58()}.json`;
  let checkpoint: IndexingCheckpoint;
  let dataStore: CoordinatorDataStore;
  try {
    const snapshotJson = await fileJsonRead(fileJson);
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
      await fileJsonWrite(fileJson, {
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
): Promise<void> {}
