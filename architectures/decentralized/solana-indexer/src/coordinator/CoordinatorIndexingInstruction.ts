import {
  JsonValue,
  Pubkey,
  casingCamelToSnake,
  jsonDecoderObject,
  jsonTypeInteger,
  jsonTypeNumber,
} from "solana-kiss";
import {
  utilsRustFixedArrayJsonDecoder,
  utilsRustSmallBooleanJsonDecoder,
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
  const signerAddress =
    instructionAddresses.get("payer") ??
    instructionAddresses.get("authority") ??
    instructionAddresses.get("user");
  if (signerAddress === undefined) {
    throw new Error("Coordinator: Instruction: Could not find signer address");
  }
  const processors = processorsByInstructionName.get(instructionName);
  if (processors !== undefined) {
    for (const processor of processors) {
      await processor(dataStore, {
        runAddress,
        signerAddress,
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
  ["update", [processAdminAction]],
  ["set_future_epoch_rates", [processAdminAction]],
  ["set_paused", [processAdminAction]],
  ["join_run", []], // TODO - how to handle join run?
  ["warmup_witness", []], // TODO - how to handle warmup witness?
  ["witness", [processWitness]],
  ["tick", []],
  ["checkpoint", []], // TODO - how to handle checkpoint?
  ["health_check", []], // TODO - how to handle health check?
  ["free_coordinator", [processAdminAction]],
]);

type ProcessingContent = {
  runAddress: Pubkey;
  signerAddress: Pubkey;
  instructionName: string;
  instructionAddresses: Map<string, Pubkey>;
  instructionPayload: JsonValue;
  ordering: bigint;
  processedTime: Date | undefined;
};

async function processAdminAction(
  dataStore: CoordinatorDataStore,
  content: ProcessingContent,
): Promise<void> {
  dataStore.saveRunAdminAction(
    content.runAddress,
    content.signerAddress,
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
  const witnessPayload = witnessArgsJsonDecoder(content.instructionPayload);
  dataStore.saveRunWitness(
    content.runAddress,
    content.signerAddress,
    content.ordering,
    content.processedTime,
    {
      position: witnessPayload.proof.position,
      index: witnessPayload.proof.index,
      witness: witnessPayload.proof.witness,
    },
    {
      step: witnessPayload.metadata.step,
      tokensPerSec: witnessPayload.metadata.tokensPerSec,
      bandwidthPerSec: witnessPayload.metadata.bandwidthPerSec,
      loss: witnessPayload.metadata.loss,
    },
  );
}

const witnessProofJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
  position: jsonTypeInteger.decoder,
  index: jsonTypeInteger.decoder,
  witness: utilsRustSmallBooleanJsonDecoder,
});

const witnessMetadataJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
  step: jsonTypeNumber.decoder,
  tokensPerSec: jsonTypeNumber.decoder,
  bandwidthPerSec: jsonTypeNumber.decoder,
  loss: jsonTypeNumber.decoder,
  promptResults: utilsRustFixedArrayJsonDecoder(jsonTypeNumber.decoder),
  promptIndex: jsonTypeNumber.decoder,
});

const witnessArgsJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
  proof: witnessProofJsonDecoder,
  metadata: witnessMetadataJsonDecoder,
});
