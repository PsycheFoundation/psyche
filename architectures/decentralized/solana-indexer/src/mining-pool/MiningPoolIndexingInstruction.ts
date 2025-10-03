import { JsonValue, Pubkey, jsonTypeObject } from "solana-kiss-data";
import { jsonTypeStringToBigint } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingInstruction(
  dataStore: MiningPoolDataStore,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: any,
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
    lenderDepositArgsJsonType.decode(instructionPayload).params
      .collateralAmount;
  dataStore.savePoolUserDeposit(poolAddress, userAddress, collateralAmount);
  dataStore.setPoolRequestOrdering(poolAddress, ordering);
}

const lenderDepositArgsJsonType = jsonTypeObject({
  params: jsonTypeObject(
    { collateralAmount: jsonTypeStringToBigint() },
    { collateralAmount: "collateral_amount" },
  ),
});
