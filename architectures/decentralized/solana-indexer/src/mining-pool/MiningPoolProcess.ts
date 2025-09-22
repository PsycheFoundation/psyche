import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import { IndexingCheckpoint } from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import {
  miningPoolDataFromJson,
  miningPoolDataToJson,
} from "./MiningPoolDataJson";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

const idlService = new ToolboxIdlService();

export async function miningPoolProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const fileJson = `mining_pool_${cluster}_${programAddress.toBase58()}.json`;
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const jsonValue = await fileJsonRead(fileJson);
    checkpoint = IndexingCheckpoint.fromJson(jsonValue.checkpoint);
    dataStore = miningPoolDataFromJson(jsonValue.dataStore);
  } catch (error) {
    console.warn("Failed to read existing mining pool JSON, starting fresh");
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new MiningPoolDataStore(new Map());
  }
  await indexingInstructionsLoop(
    endpoint,
    programAddress,
    checkpoint,
    async (
      instructionName,
      instructionAddresses,
      instructionPayload,
      ordering,
    ) => {
      if (instructionName === "lender_deposit") {
        await miningPoolProcessLenderDeposit(
          dataStore,
          instructionAddresses,
          instructionPayload,
          ordering,
        );
      }
    },
    async (checkpoint) => {
      for (const [poolAddress, poolValue] of dataStore.getPools()) {
        if (poolValue.latestAccountState === undefined) {
          const accountInfo = await idlService.getAndInferAndDecodeAccount(
            endpoint,
            new PublicKey(poolAddress),
          );
          poolValue.latestAccountState = accountInfo.state;
        }
      }
      await fileJsonWrite(fileJson, {
        updatedAt: new Date().toISOString(),
        checkpoint: checkpoint.toJson(),
        dataStore: miningPoolDataToJson(dataStore),
      });
    },
  );
}

export async function miningPoolProcessLenderDeposit(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  const pool = instructionAddresses.get("pool")?.toBase58();
  if (pool === undefined) {
    throw new Error("Missing pool address");
  }
  const user = instructionAddresses.get("user")?.toBase58();
  if (user === undefined) {
    throw new Error("Missing user address");
  }
  const amount = instructionPayload?.["params"]?.["collateral_amount"];
  if (amount === undefined) {
    throw new Error("Missing collateral_amount");
  }
  dataStore.savePoolUserDeposit(pool, user, BigInt(amount), ordering);
}
