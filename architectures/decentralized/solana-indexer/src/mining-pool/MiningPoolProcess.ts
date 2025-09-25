import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  IndexingCheckpoint,
  indexingCheckpointFromJson,
  indexingCheckpointToJson,
} from "../indexing/IndexingCheckpoint";
import { indexingInstructionsLoop } from "../indexing/IndexingInstructions";
import { JsonValue } from "../json";
import {
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../jsonType";
import { saveRead, saveWrite } from "../save";
import {
  miningPoolDataFromJson,
  miningPoolDataToJson,
} from "./MiningPoolDataJson";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolProcess(
  cluster: string,
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
): Promise<void> {
  const saveName = `mining_pool_${cluster}_${programAddress.toBase58()}`;
  let checkpoint: IndexingCheckpoint;
  let dataStore: MiningPoolDataStore;
  try {
    const saveContent = await saveRead(saveName);
    checkpoint = indexingCheckpointFromJson(saveContent.checkpoint);
    dataStore = miningPoolDataFromJson(saveContent.dataStore);
    console.log("Loaded mining pool state saved from:", saveContent.updatedAt);
  } catch (error) {
    checkpoint = new IndexingCheckpoint([]);
    dataStore = new MiningPoolDataStore(new Map());
    console.warn("Failed to read existing mining pool JSON, starting fresh");
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
        await processLenderDeposit(
          dataStore,
          ordering,
          instructionAddresses,
          instructionPayload,
        );
      }
    },
    async (checkpoint) => {
      for (const poolAddress of dataStore.getInvalidatedPoolsAddresses()) {
        const accountInfo = await idlService.getAndInferAndDecodeAccount(
          endpoint,
          new PublicKey(poolAddress),
        );
        processRefreshPoolAccountState(
          dataStore,
          poolAddress,
          accountInfo.state as JsonValue,
        );
      }
      await saveWrite(saveName, {
        updatedAt: new Date().toISOString(),
        checkpoint: indexingCheckpointToJson(checkpoint),
        dataStore: miningPoolDataToJson(dataStore),
      });
    },
  );
}

const jsonTypePool = jsonTypeObject({
  bump: jsonTypeNumber(),
  index: jsonTypeStringToBigint(),
  authority: jsonTypeString(),
  collateral_mint: jsonTypeString(),
  max_deposit_collateral_amount: jsonTypeStringToBigint(),
  total_deposited_collateral_amount: jsonTypeStringToBigint(),
  total_extracted_collateral_amount: jsonTypeStringToBigint(),
  claiming_enabled: jsonTypeBoolean(),
  redeemable_mint: jsonTypeString(),
  total_claimed_redeemable_amount: jsonTypeStringToBigint(),
  freeze: jsonTypeBoolean(),
});
export async function processRefreshPoolAccountState(
  dataStore: MiningPoolDataStore,
  poolAddress: string,
  accountState: JsonValue,
): Promise<void> {
  console.log("Refreshing pool account state", poolAddress, accountState);
  const accountParsed = jsonTypePool.decode(accountState);
  dataStore.savePoolAccountState(poolAddress, accountParsed);
}

const jsonTypeLenderDepositArgs = jsonTypeObject({
  params: jsonTypeObject({
    collateral_amount: jsonTypeStringToBigint(),
  }),
});
export async function processLenderDeposit(
  dataStore: MiningPoolDataStore,
  ordering: bigint,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: JsonValue,
): Promise<void> {
  const pool = instructionAddresses.get("pool")?.toBase58();
  if (pool === undefined) {
    throw new Error("Missing pool address");
  }
  const user = instructionAddresses.get("user")?.toBase58();
  if (user === undefined) {
    throw new Error("Missing user address");
  }
  const params = jsonTypeLenderDepositArgs.decode(instructionPayload).params;
  dataStore.savePoolUserDeposit(ordering, pool, user, params.collateral_amount);
}
