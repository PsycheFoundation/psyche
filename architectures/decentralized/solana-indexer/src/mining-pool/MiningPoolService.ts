import { Application } from "express";
import { Pubkey, Solana } from "solana-kiss";
import { indexerLoop } from "../indexer/IndexerLoop";
import { saveRead, saveWrite } from "../save";
import { miningPoolApiRoutes } from "./MiningPoolApiRoutes";
import {
  MiningPoolDataStore,
  miningPoolDataStoreJsonCodec,
} from "./MiningPoolDataStore";
import { miningPoolOnCheckpoint } from "./MiningPoolOnCheckpoint";
import { miningPoolOnInstruction } from "./MiningPoolOnInstruction";

const programName = `mining_pool`;

export async function miningPoolService(
  solana: Solana,
  programAddress: Pubkey,
  expressApplication: Application,
): Promise<void> {
  const { checkpoint: initialCheckpoint, dataStore: miningPoolDataStore } =
    await saveRead(
      programAddress,
      programName,
      miningPoolDataStoreJsonCodec.decoder,
      () => new MiningPoolDataStore(programAddress, new Map()),
    );
  miningPoolApiRoutes(programAddress, expressApplication, miningPoolDataStore);
  await indexerLoop(
    solana,
    programAddress,
    initialCheckpoint,
    async ({ updatedCheckpoint, discoveredInstructions }) => {
      for (const instruction of discoveredInstructions) {
        miningPoolOnInstruction(miningPoolDataStore, instruction);
      }
      await miningPoolOnCheckpoint(solana, miningPoolDataStore);
      await saveWrite(
        programAddress,
        programName,
        updatedCheckpoint,
        miningPoolDataStore,
        miningPoolDataStoreJsonCodec.encoder,
      );
    },
  );
}
