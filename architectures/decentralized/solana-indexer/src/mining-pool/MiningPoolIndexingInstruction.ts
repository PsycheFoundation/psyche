import { PublicKey } from "@solana/web3.js";
import { jsonTypeObject, jsonTypeStringToBigint, JsonValue } from "../json";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingInstruction(
  dataStore: MiningPoolDataStore,
  instructionName: string,
  instructionAddresses: Map<string, PublicKey>,
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

const jsonTypeLenderDepositArgs = jsonTypeObject({
  params: jsonTypeObject({
    collateralAmount: jsonTypeStringToBigint(),
  }),
});
export async function miningPoolIndexingInstructionLenderDeposit(
  dataStore: MiningPoolDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  const poolAddress = instructionAddresses.get("pool")?.toBase58();
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing pool address",
    );
  }
  const userAddress = instructionAddresses.get("user")?.toBase58();
  if (userAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: LenderDeposit: Missing user address",
    );
  }
  const params = jsonTypeLenderDepositArgs.decode(instructionPayload).params;
  dataStore.savePoolUserDeposit(
    poolAddress,
    userAddress,
    params.collateralAmount,
    ordering,
  );
  dataStore.invalidatePoolAccountState(poolAddress, ordering);
}
