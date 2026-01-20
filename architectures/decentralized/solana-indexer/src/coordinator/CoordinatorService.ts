import { Application } from "express";
import { Pubkey, Solana } from "solana-kiss";
import { indexerLoop } from "../indexer/IndexerLoop";
import { saveRead, saveWrite } from "../save";
import { coordinatorApiRoutes } from "./CoordinatorApiRoutes";
import {
  CoordinatorDataStore,
  coordinatorDataStoreJsonCodec,
} from "./CoordinatorDataStore";
import { coordinatorOnCheckpoint } from "./CoordinatorOnCheckpoint";
import { coordinatorOnInstruction } from "./CoordinatorOnInstruction";

const programName = `coordinator`;

export async function coordinatorService(
  solana: Solana,
  programAddress: Pubkey,
  expressApp: Application,
) {
  const { checkpoint: initialCheckpoint, dataStore: coordinatorDataStore } =
    await saveRead(
      programAddress,
      programName,
      coordinatorDataStoreJsonCodec.decoder,
      () => new CoordinatorDataStore(programAddress, new Map()),
    );
  coordinatorApiRoutes(programAddress, expressApp, coordinatorDataStore);
  await indexerLoop(
    solana,
    programAddress,
    initialCheckpoint,
    async ({ updatedCheckpoint, discoveredInstructions }) => {
      for (const instruction of discoveredInstructions) {
        await coordinatorOnInstruction(coordinatorDataStore, instruction);
      }
      await coordinatorOnCheckpoint(solana, coordinatorDataStore);
      await saveWrite(
        programAddress,
        programName,
        updatedCheckpoint,
        coordinatorDataStore,
        coordinatorDataStoreJsonCodec.encoder,
      );
    },
  );
}
