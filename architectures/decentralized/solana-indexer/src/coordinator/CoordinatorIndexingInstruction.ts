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
  const runAddress = instructionAddresses
    .get("coordinator_account")
    ?.toBase58();
  if (runAddress === undefined) {
    throw new Error("Coordinator: Instruction: Missing run address");
  }
  dataStore.setRunRequestOrdering(runAddress, ordering);

  if (instructionName === "witness") {
    await coordinatorIndexingInstructionWitness(
      dataStore,
      runAddress,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  }
  //console.log(instructionName, instructionPayload);
}

async function coordinatorIndexingInstructionWitness(
  dataStore: CoordinatorDataStore,
  runAddress: string,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  const userAddress = instructionAddresses.get("user")?.toBase58();
  if (userAddress === undefined) {
    throw new Error("Coordinator: Instruction: Witness: Missing user address");
  }
  const metadata = witnessArgsJsonType.decode(instructionPayload).metadata;
  dataStore.saveRunWitness(runAddress, userAddress, ordering, {
    step: metadata.step,
    tokensPerSec: metadata.tokensPerSec,
    bandwidthPerSec: metadata.bandwidthPerSec,
    loss: metadata.loss,
  });
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
