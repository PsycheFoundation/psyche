import { jsonTypeInteger, JsonValue, Pubkey } from "solana-kiss-data";
import { utilsObjectSnakeCaseJsonDecoder } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingInstruction(
  dataStore: MiningPoolDataStore,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
  processedTime: Date | undefined,
) {
  const poolAddress = instructionAddresses.get("pool");
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: PoolExtract: Missing pool address",
    );
  }
  const processors = processorsByInstructionName.get(instructionName);
  if (processors !== undefined) {
    for (const processor of processors) {
      await processor(dataStore, {
        poolAddress,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
        processedTime,
      });
    }
  } else {
    console.warn("MiningPool: Unknown instruction:", instructionName);
  }
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

const processorsByInstructionName = new Map([
  ["pool_create", [processAdminAction]],
  ["pool_update", [processAdminAction]],
  ["pool_extract", [processAdminAction, processPoolExtract]],
  ["pool_claimable", [processAdminAction]],
  ["lender_create", []],
  ["lender_deposit", [processLenderDeposit]],
  ["lender_claim", [processLenderClaim]],
]);

type ProcessingContent = {
  poolAddress: Pubkey;
  instructionName: string;
  instructionAddresses: Map<string, Pubkey>;
  instructionPayload: JsonValue;
  ordering: bigint;
  processedTime: Date | undefined;
};

async function processAdminAction(
  dataStore: MiningPoolDataStore,
  content: ProcessingContent,
): Promise<void> {
  dataStore.savePoolAdminAction(
    content.poolAddress,
    content.instructionName,
    content.instructionAddresses,
    content.instructionPayload,
    content.ordering,
    content.processedTime,
  );
}

async function processPoolExtract(
  dataStore: MiningPoolDataStore,
  content: ProcessingContent,
): Promise<void> {
  const instructionParams = poolExtractArgsJsonDecoder(
    content.instructionPayload,
  ).params;
  dataStore.savePoolExtract(
    content.poolAddress,
    instructionParams.collateralAmount,
  );
}

async function processLenderDeposit(
  dataStore: MiningPoolDataStore,
  content: ProcessingContent,
): Promise<void> {
  const userAddress = content.instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const instructionParams = lenderDepositArgsJsonDecoder(
    content.instructionPayload,
  ).params;
  dataStore.savePoolDeposit(
    content.poolAddress,
    userAddress,
    instructionParams.collateralAmount,
  );
}

async function processLenderClaim(
  dataStore: MiningPoolDataStore,
  content: ProcessingContent,
): Promise<void> {
  const userAddress = content.instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const instructionParams = lenderClaimArgsJsonDecoder(
    content.instructionPayload,
  ).params;
  dataStore.savePoolClaim(
    content.poolAddress,
    userAddress,
    instructionParams.redeemableAmount,
  );
}

const poolExtractArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    collateralAmount: jsonTypeInteger.decoder,
  }),
});

const lenderDepositArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    collateralAmount: jsonTypeInteger.decoder,
  }),
});

const lenderClaimArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    redeemableAmount: jsonTypeInteger.decoder,
  }),
});
