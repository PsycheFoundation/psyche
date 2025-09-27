import { PublicKey } from "@solana/web3.js";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingInstruction(
  dataStore: CoordinatorDataStore,
  instructionName: string,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  if (instructionName === "tick") {
    return await coordinatorIndexingInstructionTick(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  } else if (instructionName === "witness") {
    return await coordinatorIndexingInstructionWitness(
      dataStore,
      instructionAddresses,
      instructionPayload,
      ordering,
    );
  } else {
    console.warn("Unknown instruction", instructionName);
  }

  const runAddress = instructionAddresses
    .get("coordinator_account")
    ?.toBase58();
  if (runAddress === undefined) {
    throw new Error("Coordinator: Instruction: Missing run address");
  }
  dataStore.invalidateRunAccountState(runAddress, ordering);
}

export async function coordinatorIndexingInstructionTick(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  console.log("tick", instructionPayload);
}

export async function coordinatorIndexingInstructionWitness(
  dataStore: CoordinatorDataStore,
  instructionAddresses: Map<string, PublicKey>,
  instructionPayload: any,
  ordering: bigint,
): Promise<void> {
  console.log("witness", instructionPayload.metadata);
  console.log(
    "eval",
    JSON.stringify(instructionPayload.metadata.evals, null, 2),
  );
}
