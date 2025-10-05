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
  processedTime: Date | undefined,
): Promise<void> {
  const runAddress = instructionAddresses.get("coordinator_account");
  if (runAddress === undefined) {
    throw new Error("Coordinator: Instruction: Missing run address");
  }
  const processors = processorsByInstructionName.get(instructionName);
  if (processors !== undefined) {
    for (const processor of processors) {
      await processor(dataStore, {
        runAddress,
        instructionName,
        instructionAddresses,
        instructionPayload,
        ordering,
        processedTime,
      });
    }
  } else {
    console.warn("Coordinator: Unknown instruction:", instructionName);
  }
  dataStore.setRunRequestOrdering(runAddress, ordering);
}

const processorsByInstructionName = new Map([
  ["init_coordinator", [processAdminAction]],
  ["tick", []],
  ["witness", [processWitness]],
  ["warmup_witness", []], // TODO - how to handle warmup witness?
  ["set_paused", [processAdminAction]],
  ["update", [processAdminAction]],
  ["join_run", []], // TODO - how to handle join run?
  ["checkpoint", []], // TODO - how to handle checkpoint?
  ["health_check", []], // TODO - how to handle health check?
  ["set_future_epoch_rates", [processAdminAction]],
  ["free_coordinator", [processAdminAction]],
]);

type ProcessingContent = {
  runAddress: Pubkey;
  instructionName: string;
  instructionAddresses: Map<string, Pubkey>;
  instructionPayload: JsonValue;
  ordering: bigint;
  processedTime: Date | undefined;
};

export async function processAdminAction(
  dataStore: CoordinatorDataStore,
  content: ProcessingContent,
): Promise<void> {
  dataStore.saveRunAdminAction(
    content.runAddress,
    content.instructionName,
    content.instructionAddresses,
    content.instructionPayload,
    content.ordering,
    content.processedTime,
  );
}

async function processWitness(
  dataStore: CoordinatorDataStore,
  content: ProcessingContent,
): Promise<void> {
  const userAddress = content.instructionAddresses.get("user");
  if (userAddress === undefined) {
    throw new Error("Coordinator: Instruction: Witness: Missing user address");
  }
  const witnessMetadata = witnessArgsJsonDecoder(
    content.instructionPayload,
  ).metadata;
  if (witnessMetadata.loss === null) {
    throw new Error("Coordinator: Instruction: Witness: Missing loss");
  }
  dataStore.saveRunWitness(
    content.runAddress,
    userAddress,
    content.ordering,
    content.processedTime,
    {
      step: witnessMetadata.step,
      tokensPerSec: witnessMetadata.tokensPerSec,
      bandwidthPerSec: witnessMetadata.bandwidthPerSec,
      loss: witnessMetadata.loss,
    },
  );
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
