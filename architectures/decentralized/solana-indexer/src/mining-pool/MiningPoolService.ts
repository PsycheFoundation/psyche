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
  MiningPoolDataStore,
  miningPoolDataStoreJsonType,
} from "./MiningPoolDataStore";
import { miningPoolIndexingCheckpoint } from "./MiningPoolIndexingCheckpoint";
import { miningPoolIndexingInstruction } from "./MiningPoolIndexingInstruction";

export async function miningPoolService(
  cluster: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
): Promise<void> {
  const saveName = `mining_pool_${cluster}_${programAddress}`;
  const { checkpoint, dataStore } = await miningPoolServiceLoader(saveName);
  // TODO - add API calls here to serve data from dataStore
  await miningPoolServiceIndexing(
    saveName,
    rpcHttp,
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
    checkpoint = indexingCheckpointJsonType.decoder(saveContent.checkpoint);
    dataStore = miningPoolDataStoreJsonType.decoder(saveContent.dataStore);
    console.log("Loaded mining pool state from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = { indexedChunks: [] };
    dataStore = new MiningPoolDataStore(new Map());
    console.warn("Failed to read existing mining pool JSON, starting fresh");
  }
  return { checkpoint, dataStore };
}

export async function miningPoolServiceIndexing(
  saveName: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: MiningPoolDataStore,
) {
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
      await miningPoolIndexingInstruction(
        dataStore,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
      );
    },
    async (checkpoint) => {
      await miningPoolIndexingCheckpoint(rpcHttp, programIdl, dataStore);
      await saveWrite(saveName, {
        checkpoint: indexingCheckpointJsonType.encoder(checkpoint),
        dataStore: miningPoolDataStoreJsonType.encoder(dataStore),
      });
    },
  );
}
