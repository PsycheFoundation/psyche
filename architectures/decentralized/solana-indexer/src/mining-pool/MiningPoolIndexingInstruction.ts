import { jsonTypeInteger, JsonValue, Pubkey } from "solana-kiss-data";
import { utilsObjectSnakeCaseJsonDecoder } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingInstruction(
  dataStore: MiningPoolDataStore,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
) {
  const poolAddress = instructionAddresses.get("pool");
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: PoolExtract: Missing pool address",
    );
  }
  if (instructionName === "lender_deposit") {
    await instructionLenderDeposit(
      dataStore,
      poolAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  if (instructionName === "lender_claim") {
    await instructionLenderClaim(
      dataStore,
      poolAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  if (instructionName === "pool_extract") {
    await instructionPoolExtract(
      dataStore,
      poolAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  if (instructionName === "pool_update") {
    await instructionPoolUpdate(
      dataStore,
      poolAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  if (instructionName === "pool_claimable") {
    await instructionPoolClaimable(
      dataStore,
      poolAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
}

export async function instructionPoolExtract(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  const instructionParams =
    poolExtractArgsJsonDecoder(instructionPayload).params;
  dataStore.savePoolExtract(poolAddress, instructionParams.collateralAmount);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function instructionPoolUpdate(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  dataStore.savePoolUpdate(poolAddress, ordering, instructionPayload);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function instructionPoolClaimable(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  _instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  dataStore.savePoolClaimable(poolAddress, ordering, instructionPayload);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function instructionLenderDeposit(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
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
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function instructionLenderClaim(
  dataStore: MiningPoolDataStore,
  poolAddress: Pubkey,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
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
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
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
