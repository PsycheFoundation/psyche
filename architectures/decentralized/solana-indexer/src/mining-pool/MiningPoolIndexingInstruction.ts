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
  console.log("instructionName", instructionName, instructionPayload);
  if (instructionName === "lender_deposit") {
    await miningPoolIndexingInstructionLenderDeposit(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
}

export async function miningPoolIndexingInstructionLenderDeposit(
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

const lenderDepositArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  params: utilsObjectSnakeCaseJsonDecoder({
    collateralAmount: utilsBigintStringJsonType.decoder,
  }),
});
