import {
  CoordinatorRunAnalysis,
  IndexedInstruction,
} from "psyche-indexer-codecs";
import {
  Pubkey,
  jsonCodecBigInt,
  jsonCodecNumber,
  jsonDecoderNullable,
  jsonDecoderObjectToObject,
} from "solana-kiss";
import {
  jsonDecoderRustClientId,
  jsonDecoderRustFixedArray,
  jsonDecoderRustFixedString,
  jsonDecoderRustSmallBoolean,
} from "../json";
import { utilsBigintArraySortAscending } from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export async function coordinatorOnInstruction(
  dataStore: CoordinatorDataStore,
  instruction: IndexedInstruction,
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
  const runAnalysis = dataStore.getRunAnalysis(runAddress);
  const processors = processorsByInstructionName.get(
    instruction.instructionName,
  );
  if (processors !== undefined) {
    for (const processor of processors) {
      await processor(runAnalysis, {
        runAddress,
        signerAddress,
        instruction,
      });
    }
  } else {
    console.warn(
      "Coordinator: Unknown instruction:",
      instruction.instructionName,
    );
  }
  if (instruction.instructionOrdinal > runAnalysis.latestKnownChangeOrdinal) {
    runAnalysis.latestKnownChangeOrdinal = instruction.instructionOrdinal;
  }
}

const processorsByInstructionName = new Map([
  ["init_coordinator", [processAdminAction]],
  ["update", [processAdminAction]],
  ["set_future_epoch_rates", [processAdminAction]],
  ["set_paused", [processAdminAction]],
  ["update_client_version", [processAdminAction]],
  ["tick", []],
  ["warmup_witness", []],
  ["health_check", []],
  ["join_run", [processJoinRun]],
  ["checkpoint", [processCheckpoint]],
  ["witness", [processWitness]],
  ["free_coordinator", [processAdminAction, processFinish]],
]);

type ProcessingContext = {
  runAddress: Pubkey;
  signerAddress: Pubkey;
  instruction: IndexedInstruction;
};

async function processAdminAction(
  runAnalysis: CoordinatorRunAnalysis,
  context: ProcessingContext,
): Promise<void> {
  runAnalysis.adminHistory.push(context.instruction);
  utilsBigintArraySortAscending(
    runAnalysis.adminHistory,
    (adminInstruction) => adminInstruction.instructionOrdinal,
  );
}

async function processJoinRun(
  runAnalysis: CoordinatorRunAnalysis,
  context: ProcessingContext,
) {
  const joinRunPayload = joinRunJsonDecoder(
    context.instruction.instructionPayload,
  );
  runAnalysis.joinHistory.push({
    blockTime: context.instruction.blockTime,
    instructionOrdinal: context.instruction.instructionOrdinal,
    user: context.signerAddress,
    p2pIdentity: joinRunPayload.params.clientId.p2pIdentity,
  });
  utilsBigintArraySortAscending(
    runAnalysis.joinHistory,
    (joinRunInstruction) => joinRunInstruction.instructionOrdinal,
  );
}

async function processCheckpoint(
  runAnalysis: CoordinatorRunAnalysis,
  context: ProcessingContext,
) {
  const checkpointPayload = checkpointJsonDecoder(
    context.instruction.instructionPayload,
  );
  runAnalysis.checkpointHistory.push({
    blockTime: context.instruction.blockTime,
    instructionOrdinal: context.instruction.instructionOrdinal,
    user: context.signerAddress,
    repo: checkpointPayload.repo,
  });
  utilsBigintArraySortAscending(
    runAnalysis.checkpointHistory,
    (checkpointInstruction) => checkpointInstruction.instructionOrdinal,
  );
}

async function processFinish(
  runAnalysis: CoordinatorRunAnalysis,
  context: ProcessingContext,
): Promise<void> {
  runAnalysis.finishesOrdinals.push(context.instruction.instructionOrdinal);
}

async function processWitness(
  runAnalysis: CoordinatorRunAnalysis,
  context: ProcessingContext,
): Promise<void> {
  const witnessPayload = witnessJsonDecoder(
    context.instruction.instructionPayload,
  );
  if (!witnessPayload.proof.witness) {
    return;
  }
  const witnessUser = context.signerAddress;
  const witnessOrdinal = context.instruction.instructionOrdinal;
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
      time: context.instruction.blockTime,
    });
  }
}

const joinRunJsonDecoder = jsonDecoderObjectToObject({
  params: jsonDecoderObjectToObject({
    clientId: jsonDecoderRustClientId,
  }),
});

const checkpointJsonDecoder = jsonDecoderObjectToObject({
  repo: jsonDecoderObjectToObject({
    repoId: jsonDecoderRustFixedString,
    revision: jsonDecoderNullable(jsonDecoderRustFixedString),
  }),
});

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
