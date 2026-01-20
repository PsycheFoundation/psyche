import {
  jsonCodecBigInt,
  jsonCodecNumber,
  jsonCodecPubkey,
  jsonCodecString,
  jsonCodecValue,
  jsonDecoderObjectToObject,
  Pubkey,
  Solana,
} from "solana-kiss";
import {
  jsonDecoderRustClientId,
  jsonDecoderRustFixedArray,
  jsonDecoderRustFixedString,
  jsonDecoderRustSmallBoolean,
} from "../json";
import {
  utilRunInParallel,
  utilsBigintArraySortAscending,
  utilsPlotPoints,
} from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";
import {
  CoordinatorDataRunAnalysis,
  CoordinatorDataRunSample,
} from "./CoordinatorDataTypes";

export async function coordinatorOnCheckpoint(
  solana: Solana,
  dataStore: CoordinatorDataStore,
) {
  const tasks = await utilRunInParallel(
    dataStore.runAnalysisByAddress.entries(),
    async ([runAddress, runAnalysis]) => {
      return await runCheckpoint(
        solana,
        dataStore.programAddress,
        runAddress,
        runAnalysis,
      );
    },
  );
  for (const task of tasks) {
    if (task.result.error) {
      console.error(
        "Failed to process run checkpoint",
        task.input[0],
        task.result.error,
      );
    }
  }
}

async function runCheckpoint(
  solana: Solana,
  programAddress: Pubkey,
  runAddress: Pubkey,
  runAnalysis: CoordinatorDataRunAnalysis,
) {
  if (
    runAnalysis.latestUpdateFetchOrdinal ===
    runAnalysis.latestKnownChangeOrdinal
  ) {
    return;
  }
  runAnalysis.latestUpdateFetchOrdinal = runAnalysis.latestKnownChangeOrdinal;
  try {
    await fetchAndUpdateOnchainState(solana, runAddress, runAnalysis);
  } catch (error) {
    console.error("Failed to refresh run state", runAddress, error);
  }
  for (const [statName, statSamples] of runAnalysis.samplesByStatName) {
    aggregateStatSamples(
      programAddress,
      runAddress,
      runAnalysis.latestOnchainSnapshot?.parsed.runId,
      statName,
      statSamples,
      runAnalysis.finishesOrdinals,
    );
  }
}

function aggregateStatSamples(
  programAddress: Pubkey,
  runAddress: Pubkey,
  runId: string | undefined,
  statName: string,
  statSamples: Array<CoordinatorDataRunSample>,
  finishesOrdinals: Array<bigint>,
) {
  utilsBigintArraySortAscending(statSamples, (sample) => sample.maxOrdinal);
  utilsBigintArraySortAscending(
    finishesOrdinals,
    (finishOrdinal) => finishOrdinal,
  );
  for (
    let sliceIndex = 0;
    sliceIndex <= finishesOrdinals.length;
    sliceIndex++
  ) {
    const prevOrdinal = finishesOrdinals[sliceIndex - 1];
    const nextOrdinal = finishesOrdinals[sliceIndex];
    let sampleIndexMin = 0;
    if (prevOrdinal !== undefined) {
      while (
        sampleIndexMin < statSamples.length &&
        statSamples[sampleIndexMin]!.maxOrdinal < prevOrdinal
      ) {
        sampleIndexMin++;
      }
    }
    let sampleIndexMax = statSamples.length - 1;
    if (nextOrdinal !== undefined) {
      while (
        sampleIndexMax >= sampleIndexMin &&
        statSamples[sampleIndexMax]!.maxOrdinal > nextOrdinal
      ) {
        sampleIndexMax--;
      }
    }
    aggregateStatSamplesSlice(
      programAddress,
      runAddress,
      runId,
      statName,
      statSamples,
      sliceIndex,
      sampleIndexMin,
      sampleIndexMax,
    );
  }
  if (runId || statSamples.length > 1000) {
    utilsPlotPoints(
      `${programAddress}`,
      runId ? `run ${runId}` : `run ${runAddress}`,
      `history (${statName})`,
      statSamples.map((sample) => ({
        x: sample.time?.getTime() ?? NaN,
        y: sample.sumValue / sample.numValue,
      })),
      (x) => new Date(x).toISOString(),
    );
  }
}

function aggregateStatSamplesSlice(
  programAddress: Pubkey,
  runAddress: Pubkey,
  runId: string | undefined,
  statName: string,
  statSamples: Array<CoordinatorDataRunSample>,
  sliceIndex: number,
  sampleIndexMin: number,
  sampleIndexMax: number,
) {
  let minStep = Infinity;
  let maxStep = 0;
  for (
    let sampleIndex = sampleIndexMax - 1;
    sampleIndex >= sampleIndexMin;
    sampleIndex--
  ) {
    const prevIndex = sampleIndex;
    const nextIndex = sampleIndex + 1;
    const prevSample = statSamples[prevIndex]!;
    const nextSample = statSamples[nextIndex]!;
    minStep = Math.min(minStep, nextSample.step);
    maxStep = Math.max(maxStep, nextSample.step);
    if (prevSample.step === nextSample.step) {
      nextSample.sumValue += prevSample.sumValue;
      nextSample.numValue += prevSample.numValue;
      statSamples.splice(prevIndex, 1);
      sampleIndexMax--;
    }
  }
  let chunkSize = 1;
  while (chunkSize * 2000 < maxStep - minStep) {
    chunkSize *= 2;
  }
  for (
    let sampleIndex = sampleIndexMax - 1;
    sampleIndex >= sampleIndexMin;
    sampleIndex--
  ) {
    if ((statSamples[sampleIndex]!.step - 1) % chunkSize !== 0) {
      statSamples.splice(sampleIndex, 1);
      sampleIndexMax--;
    }
  }
  if (runId || statSamples.length > 1000) {
    utilsPlotPoints(
      `${programAddress}`,
      runId ? `run ${runId}` : `run ${runAddress}`,
      `s${sliceIndex} (${statName})`,
      statSamples.slice(sampleIndexMin, sampleIndexMax + 1).map((sample) => ({
        x: sample.step,
        y: sample.sumValue / sample.numValue,
      })),
      (x) => `Step ${x}`,
    );
  }
}

async function fetchAndUpdateOnchainState(
  solana: Solana,
  runAddress: Pubkey,
  runAnalysis: CoordinatorDataRunAnalysis,
) {
  const { accountState: runInstanceState } =
    await solana.getAndInferAndDecodeAccount(runAddress);
  const runInstanceParsed = runInstanceJsonDecoder(runInstanceState);
  const runAccountAddress = runInstanceParsed.coordinatorAccount;
  const { accountState: runAccountState } =
    await solana.getAndInferAndDecodeAccount(runAccountAddress);
  const runAccountParsed = runAccountJsonDecoder(runAccountState);
  runAnalysis.latestOnchainSnapshot = {
    parsed: {
      runId: runInstanceParsed.runId,
      mainAuthority: runInstanceParsed.mainAuthority,
      joinAuthority: runInstanceParsed.joinAuthority,
      name: runAccountParsed.state.metadata.name,
      description: runAccountParsed.state.metadata.description,
      status: runAccountParsed.state.coordinator.runState,
      numParameters: runAccountParsed.state.metadata.numParameters,
      joinedClients: runAccountParsed.state.clientsState.clients.map(
        (client) => ({
          signer: client.id.signer,
          earned: client.earned,
          slashed: client.slashed,
        }),
      ),
      epochClients: runAccountParsed.state.coordinator.epochState.clients.map(
        (client) => ({
          signer: client.id.signer,
          state: client.state,
        }),
      ),
      progress: {
        epoch: runAccountParsed.state.coordinator.progress.epoch,
        step: runAccountParsed.state.coordinator.progress.step,
      },
    },
    native: {
      coordinatorInstance: runInstanceState,
      coordinatorAccount: runAccountState,
    },
    updatedAt: new Date(),
  };
}

const runInstanceJsonDecoder = jsonDecoderObjectToObject({
  bump: jsonCodecNumber.decoder,
  mainAuthority: jsonCodecPubkey.decoder,
  joinAuthority: jsonCodecPubkey.decoder,
  coordinatorAccount: jsonCodecPubkey.decoder,
  runId: jsonCodecString.decoder,
});

const runAccountJsonDecoder = jsonDecoderObjectToObject({
  nonce: jsonCodecBigInt.decoder,
  state: jsonDecoderObjectToObject({
    metadata: jsonDecoderObjectToObject({
      name: jsonDecoderRustFixedString,
      description: jsonDecoderRustFixedString,
      numParameters: jsonCodecBigInt.decoder,
      vocabSize: jsonCodecBigInt.decoder,
    }),
    coordinator: jsonDecoderObjectToObject({
      runId: jsonDecoderRustFixedString,
      runState: jsonCodecString.decoder,
      /*
      model: jsonCodecValue.decoder,
      config: jsonDecoderObjectToObject({
        warmupTime: jsonCodecBigInt.decoder,
        cooldownTime: jsonCodecBigInt.decoder,
        maxRoundTrainTime: jsonCodecBigInt.decoder,
        roundWitnessTime: jsonCodecBigInt.decoder,
        globalBatchSizeWarmupTokens: jsonCodecBigInt.decoder,
        roundsPerEpoch: jsonCodecNumber.decoder,
        totalSteps: jsonCodecNumber.decoder,
        initMinClients: jsonCodecNumber.decoder,
        minClients: jsonCodecNumber.decoder,
        witnessNodes: jsonCodecNumber.decoder,
        globalBatchSizeStart: jsonCodecNumber.decoder,
        globalBatchSizeEnd: jsonCodecNumber.decoder,
        verificationPercent: jsonCodecNumber.decoder,
      }),
      */
      progress: jsonDecoderObjectToObject({
        epoch: jsonCodecNumber.decoder,
        step: jsonCodecNumber.decoder,
        epochStartDataIndex: jsonCodecBigInt.decoder,
      }),
      epochState: jsonDecoderObjectToObject({
        rounds: jsonCodecValue.decoder,
        clients: jsonDecoderRustFixedArray(
          jsonDecoderObjectToObject({
            id: jsonDecoderRustClientId,
            exitedHeight: jsonCodecNumber.decoder,
            state: jsonCodecString.decoder,
          }),
        ),
        /*
        exitedClients: jsonDecoderRustFixedArray(
          jsonDecoderObjectToObject({
            id: jsonDecoderRustClientId,
            exitedHeight: jsonCodecNumber.decoder,
            state: jsonCodecString.decoder,
          }),
        ),
        roundsHead: jsonCodecNumber.decoder,
        startStep: jsonCodecNumber.decoder,
        firstRound: jsonDecoderRustSmallBoolean,
        checkpointed: jsonDecoderRustSmallBoolean,
        coldStartEpoch: jsonDecoderRustSmallBoolean,
        */
      }),
      // runStateStartUnixTimestamp: jsonCodecBigInt.decoder,
      pendingPause: jsonDecoderRustSmallBoolean,
    }),
    clientsState: jsonDecoderObjectToObject({
      nextActive: jsonCodecBigInt.decoder,
      clients: jsonDecoderRustFixedArray(
        jsonDecoderObjectToObject({
          active: jsonCodecBigInt.decoder,
          earned: jsonCodecBigInt.decoder,
          slashed: jsonCodecBigInt.decoder,
          id: jsonDecoderRustClientId,
        }),
      ),
      /*
      currentEpochRates: jsonDecoderObjectToObject({
        earningRate: jsonCodecBigInt.decoder,
        slashingRate: jsonCodecBigInt.decoder,
      }),
      futureEpochRates: jsonDecoderObjectToObject({
        earningRate: jsonCodecBigInt.decoder,
        slashingRate: jsonCodecBigInt.decoder,
      }),
      */
    }),
    //isWarmupFirstTick: jsonDecoderRustSmallBoolean,
    //isTrainingFirstTick: jsonDecoderRustSmallBoolean,
  }),
});
