import { PublicKey } from "@solana/web3.js";
import { jsonTypeNumber } from "../json";
import { jsonTypeObjectSnakeCase, jsonTypeRustFixedArray } from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingInstruction(
  dataStore: CoordinatorDataStore,
  instructionName: string,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  if (instructionName === "witness") {
    await coordinatorIndexingInstructionWitness(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  //console.log(instructionName, instructionPayload);

  const runAddress = instructionAddresses
    .get("coordinator_account")
    ?.toBase58();
  if (runAddress === undefined) {
    throw new Error("Coordinator: Instruction: Missing run address");
  }
  dataStore.setRunRequestOrdering(runAddress, ordering);
}

export async function coordinatorIndexingInstructionTick(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {}

export async function coordinatorIndexingInstructionWitness(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  const user = instructionAddresses.get("user")?.toBase58();
  if (user === undefined) {
    throw new Error("Coordinator: Instruction: Witness: Missing user address");
  }
  const dudu = witnessArgsJsonType.decode(instructionPayload);
  console.log("Witness", dudu);
}

const witnessArgsJsonType = jsonTypeObjectSnakeCase({
  metadata: jsonTypeObjectSnakeCase({
    step: jsonTypeNumber(),
    tokensPerSec: jsonTypeNumber(),
    bandwidthPerSec: jsonTypeNumber(),
    loss: jsonTypeNumber(),
    promptResults: jsonTypeRustFixedArray(jsonTypeNumber()),
    promptIndex: jsonTypeNumber(),
  }),
});
