import { Application } from "express";
import { Pubkey, pubkeyToBase58, RpcHttp } from "solana-kiss";
import {
  IndexingCheckpoint,
  indexingCheckpointJsonCodec,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructions } from "../indexing/IndexingInstructions";
import { saveExists, saveRead, saveWrite } from "../save";
import { utilsGetProgramAnchorIdl } from "../utils";
import {
  MiningPoolDataStore,
  miningPoolDataStoreJsonCodec,
} from "./MiningPoolDataStore";
import { miningPoolEndpoint } from "./MiningPoolEndpoint";
import { miningPoolIndexingCheckpoint } from "./MiningPoolIndexingOnCheckpoint";
import { miningPoolIndexingOnInstruction } from "./MiningPoolIndexingOnInstruction";

export async function miningPoolService(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  expressApp: Application,
): Promise<void> {
  const { checkpoint, dataStore } = await serviceLoader(programAddress);
  miningPoolEndpoint(programAddress, expressApp, dataStore);
  await serviceIndexing(rpcHttp, programAddress, checkpoint, dataStore);
}

async function serviceLoader(programAddress: Pubkey) {
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const saveContent = await saveRead(
      pubkeyToBase58(programAddress),
      saveName,
    );
    checkpoint = indexingCheckpointJsonCodec.decoder(saveContent.checkpoint);
    dataStore = miningPoolDataStoreJsonCodec.decoder(saveContent.dataStore);
    console.log("Loaded mining pool state from:", saveContent.updatedAt);
  } catch (error) {
    const willOverride = await saveExists(
      pubkeyToBase58(programAddress),
      saveName,
    );
    if (willOverride && !process.env["ALLOW_STATE_OVERRIDE"]) {
      throw new Error(
        "Failed to read existing mining pool JSON, and ALLOW_STATE_OVERRIDE is not set",
      );
    }
    checkpoint = { orderedIndexedChunks: [] };
    dataStore = new MiningPoolDataStore(programAddress, new Map());
    console.warn(
      "Failed to read existing mining pool JSON, starting fresh",
      error,
    );
  }
  return { checkpoint, dataStore };
}

async function serviceIndexing(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: MiningPoolDataStore,
) {
  const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress);
  await indexingInstructions(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    programIdl,
    async ({
      blockTime,
      instructionOrdinal,
      instructionName,
      instructionAddresses,
      instructionPayload,
    }) => {
      await miningPoolIndexingOnInstruction(
        dataStore,
        blockTime,
        instructionOrdinal,
        instructionName,
        instructionAddresses,
        instructionPayload,
      );
    },
    async (checkpoint) => {
      await miningPoolIndexingCheckpoint(rpcHttp, programIdl, dataStore);
      await saveWrite(pubkeyToBase58(programAddress), saveName, {
        checkpoint: indexingCheckpointJsonCodec.encoder(checkpoint),
        dataStore: miningPoolDataStoreJsonCodec.encoder(dataStore),
      });
    },
  );
}

const saveName = `mining_pool`;
