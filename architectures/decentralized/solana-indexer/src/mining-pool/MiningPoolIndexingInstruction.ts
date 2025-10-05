import { jsonTypeInteger, JsonValue, Pubkey } from "solana-kiss-data";
import { utilsObjectSnakeCaseJsonDecoder } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

const processorsByInstructionName = new Map([
  ["pool_create", [processAdminAction]],
  ["pool_update", [processAdminAction]],
  ["pool_extract", [processAdminAction, processPoolExtract]],
  ["pool_claimable", [processAdminAction]],
  ["lender_deposit", [processLenderDeposit]],
  ["lender_claim", [processLenderClaim]],
]);

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
      await processor(
        dataStore,
        poolAddress,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
        processedTime,
      );
    }
  }
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function processAdminAction(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
  processedTime: Date | undefined,
) {
  dataStore.savePoolAdminAction(
    poolAddress,
    instructionName,
    instructionAddresses,
    instructionPayload,
    ordering,
    processedTime,
  );
}

export async function processPoolExtract(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionName: string,
  _instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  _ordering: bigint,
  processedTime: Date | undefined,
): Promise<void> {
  const instructionParams =
    poolExtractArgsJsonDecoder(instructionPayload).params;
  dataStore.savePoolExtract(poolAddress, instructionParams.collateralAmount);
}

export async function processLenderDeposit(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  _ordering: bigint,
  _processedTime: Date | undefined,
): Promise<void> {
  const userAddress = instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const instructionParams =
    lenderDepositArgsJsonDecoder(instructionPayload).params;
  dataStore.savePoolDeposit(
    poolAddress,
    userAddress,
    instructionParams.collateralAmount,
  );
}

export async function processLenderClaim(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  _ordering: bigint,
  _processedTime: Date | undefined,
): Promise<void> {
  const userAddress = instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const instructionParams =
    lenderClaimArgsJsonDecoder(instructionPayload).params;
  dataStore.savePoolClaim(
    poolAddress,
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
