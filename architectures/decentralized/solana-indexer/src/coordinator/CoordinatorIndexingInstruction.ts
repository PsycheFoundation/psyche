import { jsonTypeNumber, JsonValue, Pubkey } from "solana-kiss-data";
import {
  utilsObjectSnakeCaseJsonDecoder,
  utilsRustFixedArrayJsonDecoder,
} from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorIndexingInstruction(
  dataStore: CoordinatorDataStore,
  instructionName: string,
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  const runAddress = instructionAddresses.get("coordinator_account");
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
  instructionAddresses: Map<string, Pubkey>,
  instructionPayload: JsonValue,
  ordering: bigint,
): Promise<void> {
  const userAddress = instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error("Coordinator: Instruction: Witness: Missing user address");
  }
  const metadata = witnessArgsJsonDecoder(instructionPayload).metadata;
  dataStore.saveRunWitness(runAddress, userAddress, ordering, {
    step: metadata.step,
    tokensPerSec: metadata.tokensPerSec,
    bandwidthPerSec: metadata.bandwidthPerSec,
    loss: metadata.loss,
  });
}

const witnessArgsJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  metadata: utilsObjectSnakeCaseJsonDecoder({
    step: jsonTypeNumber.decoder,
    tokensPerSec: jsonTypeNumber.decoder,
    bandwidthPerSec: jsonTypeNumber.decoder,
    loss: jsonTypeNumber.decoder,
    promptResults: utilsRustFixedArrayJsonDecoder(jsonTypeNumber.decoder),
    promptIndex: jsonTypeNumber.decoder,
  }),
});
