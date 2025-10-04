import { JsonValue, Pubkey } from "solana-kiss-data";
import {
  utilsBigintStringJsonType,
  utilsObjectSnakeCaseJsonDecoder,
} from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingInstruction(
  dataStore: MiningPoolDataStore,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
) {
  if (instructionName === "lender_deposit") {
    await instructionLenderDeposit(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  if (instructionName === "pool_extract") {
    await instructionPoolExtract(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
}

export async function instructionPoolExtract(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  console.log("PoolExtract", instructionAddresses, instructionPayload);
  const poolAddress = instructionAddresses.get("pool");
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: PoolExtract: Missing pool address",
    );
  }
  const collateralAmount =
    poolExtractArgsJsonDecoder(instructionPayload).params.collateralAmount;
  dataStore.savePoolExtract(poolAddress, collateralAmount);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

export async function instructionLenderDeposit(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  const poolAddress = instructionAddresses.get("pool");
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing pool address",
    );
  }
  const userAddress = instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const collateralAmount =
    lenderDepositArgsJsonDecoder(instructionPayload).params.collateralAmount;
  dataStore.savePoolUserDeposit(poolAddress, userAddress, collateralAmount);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

const poolExtractArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    collateralAmount: utilsBigintStringJsonType.decoder,
  }),
});

const lenderDepositArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    collateralAmount: utilsBigintStringJsonType.decoder,
  }),
});
