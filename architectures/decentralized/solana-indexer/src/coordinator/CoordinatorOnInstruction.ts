import {
  Pubkey,
  jsonCodecBigInt,
  jsonCodecNumber,
  jsonDecoderNullable,
  jsonDecoderObjectToObject,
} from "solana-kiss";
import { IndexerInstruction } from "../indexer/IndexerTypes";
import {
  jsonDecoderRustFixedArray,
  jsonDecoderRustFixedString,
  jsonDecoderRustSmallBoolean,
} from "../json";
import { utilsBigintArraySortAscending } from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorOnInstruction(
  dataStore: CoordinatorDataStore,
  instruction: IndexerInstruction,
): Promise<void> {
  const runAddress = instruction.instructionAddresses["coordinator_instance"];
  if (runAddress === undefined) {
    throw new Error("Coordinator: Instruction: Missing run address");
  }
  const signerAddress =
    instruction.instructionAddresses["payer"] ??
    instruction.instructionAddresses["authority"] ??
    instruction.instructionAddresses["user"];
  if (signerAddress === undefined) {
    throw new Error("Coordinator: Instruction: Could not find signer address");
  }
  const processors = processorsByInstructionName.get(
    instruction.instructionName,
  );
  if (processors !== undefined) {
    for (const processor of processors) {
      await processor(dataStore, {
        runAddress,
        signerAddress,
        indexerInstruction: instruction,
      });
    }
  } else {
    console.warn(
      "Coordinator: Unknown instruction:",
      instruction.instructionName,
    );
  }
  const runAnalysis = dataStore.getRunInfo(runAddress);
  if (instruction.instructionOrdinal > runAnalysis.latestKnownChangeOrdinal) {
    runAnalysis.latestKnownChangeOrdinal = instruction.instructionOrdinal;
  }
}

const processorsByInstructionName = new Map([
  ["init_coordinator", [processImportantAction]],
  ["update", [processImportantAction]],
  ["set_future_epoch_rates", [processImportantAction]],
  ["set_paused", [processImportantAction]],
  ["update_client_version", [processImportantAction]],
  ["join_run", [processImportantAction]],
  ["warmup_witness", []], // TODO - how to handle warmup witness?
  ["witness", [processWitness]],
  ["tick", []],
  ["checkpoint", [processImportantAction]],
  ["health_check", []], // TODO - how to handle health check?
  ["free_coordinator", [processImportantAction, processFinish]],
]);

type ProcessingContext = {
  runAddress: Pubkey;
  signerAddress: Pubkey;
  indexerInstruction: IndexerInstruction;
};

async function processImportantAction(
  coordinatorDataStore: CoordinatorDataStore,
  processingContext: ProcessingContext,
): Promise<void> {
  // TODO - later on we may want to dedup important actions that are identical
  const runAnalysis = coordinatorDataStore.getRunInfo(
    processingContext.runAddress,
  );
  runAnalysis.importantHistory.push(processingContext.indexerInstruction);
  utilsBigintArraySortAscending(
    runAnalysis.importantHistory,
    (importantAction) => importantAction.instructionOrdinal,
  );
  runAnalysis.importantHistory.reverse();
}

async function processFinish(
  coordinatorDataStore: CoordinatorDataStore,
  processingContext: ProcessingContext,
): Promise<void> {
  const runAnalysis = coordinatorDataStore.getRunInfo(
    processingContext.runAddress,
  );
  runAnalysis.finishesOrdinals.push(
    processingContext.indexerInstruction.instructionOrdinal,
  );
}

async function processWitness(
  coordinatorDataStore: CoordinatorDataStore,
  context: ProcessingContext,
): Promise<void> {
  const runAnalysis = coordinatorDataStore.getRunInfo(context.runAddress);
  const witnessPayload = witnessJsonDecoder(
    context.indexerInstruction.instructionPayload,
  );
  if (!witnessPayload.proof.witness) {
    return;
  }
  const witnessUser = context.signerAddress;
  const witnessOrdinal = context.indexerInstruction.instructionOrdinal;
  const witnessStep = witnessPayload.metadata.step;
  const lastWitnessForUser = runAnalysis.lastWitnessByUser.get(witnessUser);
  if (
    lastWitnessForUser === undefined ||
    lastWitnessForUser.ordinal < witnessOrdinal
  ) {
    runAnalysis.lastWitnessByUser.set(witnessUser, {
      ordinal: witnessOrdinal,
      step: witnessStep,
    });
  }
  const witnessStats = new Map<string, number>();
  witnessStats.set("step", witnessStep);
  if (witnessPayload.metadata.bandwidthPerSec !== null) {
    witnessStats.set(
      "bandwidthPerSec",
      witnessPayload.metadata.bandwidthPerSec,
    );
  }
  if (witnessPayload.metadata.tokensPerSec !== null) {
    witnessStats.set("tokensPerSec", witnessPayload.metadata.tokensPerSec);
  }
  if (witnessPayload.metadata.efficiency !== null) {
    witnessStats.set("efficiency", witnessPayload.metadata.efficiency);
  }
  if (witnessPayload.metadata.loss !== null) {
    witnessStats.set("loss", witnessPayload.metadata.loss);
  }
  if (witnessPayload.metadata.evals !== null) {
    for (const evalItem of witnessPayload.metadata.evals) {
      witnessStats.set(evalItem.name, evalItem.value);
    }
  }
  for (const [statName, statValue] of witnessStats.entries()) {
    if (!isFinite(statValue)) {
      continue;
    }
    let statSamples = runAnalysis.samplesByStatName.get(statName);
    if (statSamples === undefined) {
      statSamples = [];
      runAnalysis.samplesByStatName.set(statName, statSamples);
    }
    statSamples.push({
      maxOrdinal: witnessOrdinal,
      step: witnessStep,
      sumValue: statValue,
      numValue: 1,
      time: context.indexerInstruction.blockTime,
    });
  }
}

const witnessProofJsonDecoder = jsonDecoderObjectToObject({
  position: jsonCodecBigInt.decoder,
  index: jsonCodecBigInt.decoder,
  witness: jsonDecoderRustSmallBoolean,
});

const witnessMetadataJsonDecoder = jsonDecoderObjectToObject({
  step: jsonCodecNumber.decoder,
  tokensPerSec: jsonDecoderNullable(jsonCodecNumber.decoder),
  bandwidthPerSec: jsonDecoderNullable(jsonCodecNumber.decoder),
  efficiency: jsonDecoderNullable(jsonCodecNumber.decoder),
  loss: jsonDecoderNullable(jsonCodecNumber.decoder),
  promptIndex: jsonDecoderNullable(jsonCodecNumber.decoder),
  promptResults: jsonDecoderNullable(
    jsonDecoderRustFixedArray(jsonCodecNumber.decoder),
  ),
  evals: jsonDecoderNullable(
    jsonDecoderRustFixedArray(
      jsonDecoderObjectToObject({
        name: jsonDecoderRustFixedString,
        value: jsonCodecNumber.decoder,
      }),
    ),
  ),
});

const witnessJsonDecoder = jsonDecoderObjectToObject({
  proof: witnessProofJsonDecoder,
  metadata: witnessMetadataJsonDecoder,
});
