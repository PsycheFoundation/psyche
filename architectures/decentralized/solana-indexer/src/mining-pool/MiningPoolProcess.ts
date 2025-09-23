import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { fileJsonRead, fileJsonWrite } from "../file";
import {
  IndexingCheckpoint,
  indexingCheckpointFromJson,
  indexingCheckpointToJson,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import {
  jsonSchemaObject,
  jsonSchemaString,
  jsonSchemaValue,
  JsonValue,
} from "../json";
import {
  miningPoolDataFromJson,
  miningPoolDataToJson,
} from "./MiningPoolDataJson";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

const saveFileJsonSchema = jsonSchemaObject({
  updatedAt: jsonSchemaString(),
  checkpoint: jsonSchemaValue(),
  dataStore: jsonSchemaValue(),
});

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
    const jsonParsed = saveFileJsonSchema.parse(jsonValue);
    checkpoint = indexingCheckpointFromJson(jsonParsed.checkpoint);
    dataStore = miningPoolDataFromJson(jsonParsed.dataStore);
    console.log(
      "Loaded mining pool state from JSON from:",
      jsonParsed.updatedAt,
    );
  } catch (error) {
    console.warn("Failed to read existing mining pool JSON, starting fresh");
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new MiningPoolDataStore(new Map());
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
        checkpoint: indexingCheckpointToJson(checkpoint),
        dataStore: miningPoolDataToJson(dataStore),
      });
    },
  );
}

const lenderParamsJsonSchema = jsonSchemaObject({
  params: jsonSchemaObject({
    collateral_amount: jsonSchemaString(),
  }),
});

export async function miningPoolProcessLenderDeposit(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: JsonValue,
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
  const payload = lenderParamsJsonSchema.parse(instructionPayload);
  const amount = BigInt(payload.params.collateral_amount);
  dataStore.savePoolUserDeposit(ordering, pool, user, amount);
}
