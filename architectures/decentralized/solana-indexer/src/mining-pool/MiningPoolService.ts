import {
  jsonTypeArray,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypePubkey,
  Pubkey,
  RpcHttp,
} from "solana-kiss";
import {
  IndexingCheckpoint,
  indexingCheckpointJsonType,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { saveRead, saveWrite } from "../save";
import { utilsGetProgramAnchorIdl } from "../utils";
import {
  MiningPoolDataStore,
  miningPoolDataStoreJsonType,
} from "./MiningPoolDataStore";
import { miningPoolIndexingCheckpoint } from "./MiningPoolIndexingCheckpoint";
import { miningPoolIndexingInstruction } from "./MiningPoolIndexingInstruction";

import { Application } from "express";
import { miningPoolDataPoolInfoJsonType } from "./MiningPoolDataPoolInfo";
import { miningPoolDataPoolStateJsonType } from "./MiningPoolDataPoolState";

export async function miningPoolService(
  cluster: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  expressApp: Application,
): Promise<void> {
  const saveName = `mining_pool_${cluster}_${programAddress}`;
  const { checkpoint, dataStore } = await serviceLoader(saveName);
  serviceEndpoint(programAddress, expressApp, dataStore);
  await serviceIndexing(
    saveName,
    rpcHttp,
    programAddress,
    checkpoint,
    dataStore,
  );
}

async function serviceLoader(saveName: string) {
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointJsonType.decoder(saveContent.checkpoint);
    dataStore = miningPoolDataStoreJsonType.decoder(saveContent.dataStore);
    console.log("Loaded mining pool state from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = { indexedChunks: [] };
    dataStore = new MiningPoolDataStore(new Map(), new Map());
    console.warn(
      "Failed to read existing mining pool JSON, starting fresh",
      error,
    );
  }
  return { checkpoint, dataStore };
}

async function serviceEndpoint(
  programAddress: Pubkey,
  expressApp: Application,
  dataStore: MiningPoolDataStore,
) {
  expressApp.get(`/mining-pool/${programAddress}/summaries`, (_, res) => {
    const poolsSummaries = [];
    for (const [poolAddress, poolInfo] of dataStore.poolInfoByAddress) {
      const poolState = poolInfo?.accountState;
      if (poolState === undefined) {
        continue;
      }
      poolsSummaries.push({ address: poolAddress, state: poolState });
    }
    return res.status(200).json(poolSummariesJsonType.encoder(poolsSummaries));
  });
  expressApp.get(`/mining-pool/${programAddress}/pool/:index`, (req, res) => {
    const poolIndex = jsonTypeInteger.decoder(req.params.index);
    const poolAddress = dataStore.poolAddressByIndex.get(poolIndex);
    if (!poolAddress) {
      return res.status(404).json({ error: "Pool address not found" });
    }
    const poolInfo = dataStore.poolInfoByAddress.get(poolAddress);
    if (!poolInfo) {
      return res.status(404).json({ error: "Pool info not found" });
    }
    return res
      .status(200)
      .json(miningPoolDataPoolInfoJsonType.encoder(poolInfo));
  });
}

async function serviceIndexing(
  saveName: string,
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  dataStore: MiningPoolDataStore,
) {
  const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress);
  await indexingInstructionsLoop(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    programIdl,
    async (
      instructionName,
      instructionAddresses,
      instructionPayload,
      context,
    ) => {
      await miningPoolIndexingInstruction(
        dataStore,
        instructionName,
        instructionAddresses,
        instructionPayload,
        context.ordering,
        context.transaction.block.time,
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

const poolSummariesJsonType = jsonTypeArray(
  jsonTypeObject((key) => key, {
    address: jsonTypePubkey,
    state: miningPoolDataPoolStateJsonType,
  }),
);
