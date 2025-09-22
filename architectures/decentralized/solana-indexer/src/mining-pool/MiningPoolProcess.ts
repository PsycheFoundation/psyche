import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import { IndexingCheckpoint } from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

function miningPoolJsonPath(
  cluster: string,
  programAddress: PublicKey,
): string {
  return `./mining_pool_${cluster}_${programAddress.toBase58()}.json`;
}

export async function miningPoolProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const jsonPath = miningPoolJsonPath(cluster, programAddress);
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const jsonValue = await fileJsonRead(jsonPath);
    checkpoint = IndexingCheckpoint.fromJson(jsonValue.checkpoint);
    dataStore = MiningPoolDataStore.fromJson(jsonValue.dataStore);
  } catch (error) {
    console.warn("Failed to read existing mining pool JSON, starting fresh");
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new MiningPoolDataStore(new Map<string, Map<string, bigint>>());
  }
  await indexingInstructionsLoop(
    endpoint,
    programAddress,
    checkpoint,
    async (instructionName, instructionAddresses, instructionPayload) => {
      if (instructionName === "lender_deposit") {
        await miningPoolProcessLenderDeposit(
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
        dataStore: dataStore.toJson(),
      });
    },
  );
}

export async function miningPoolProcessLenderDeposit(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
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
  dataStore.savePoolUserDeposit(pool, user, BigInt(amount));
}
