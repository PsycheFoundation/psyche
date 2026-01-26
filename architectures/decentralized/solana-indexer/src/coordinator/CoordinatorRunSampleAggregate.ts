import { Pubkey } from "solana-kiss";
import { utilsBigintArraySortAscending, utilsWritePointPlot } from "../utils";
import {
  CoordinatorDataRunAnalysis,
  CoordinatorDataRunSample,
} from "./CoordinatorDataTypes";

export async function coordinatorRunSampleAggregate(
  programAddress: Pubkey,
  runAnalysis: CoordinatorDataRunAnalysis,
) {
  for (const [statName, statSamples] of runAnalysis.samplesByStatName) {
    await aggregateStatSamplesCategory(
      programAddress,
      runAnalysis.latestOnchainSnapshot?.parsed.runId,
      statName,
      statSamples,
      runAnalysis.finishesOrdinals,
    );
  }
}

async function aggregateStatSamplesCategory(
  programAddress: Pubkey,
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
    let runCycleIndex = 0;
    runCycleIndex <= finishesOrdinals.length;
    runCycleIndex++
  ) {
    const prevFinishOrdinal = finishesOrdinals[runCycleIndex - 1];
    const nextFinishOrdinal = finishesOrdinals[runCycleIndex];
    let sampleIndexMin = 0;
    if (prevFinishOrdinal !== undefined) {
      while (
        sampleIndexMin < statSamples.length &&
        statSamples[sampleIndexMin]!.maxOrdinal < prevFinishOrdinal
      ) {
        sampleIndexMin++;
      }
    }
    let sampleIndexMax = statSamples.length - 1;
    if (nextFinishOrdinal !== undefined) {
      while (
        sampleIndexMax >= sampleIndexMin &&
        statSamples[sampleIndexMax]!.maxOrdinal > nextFinishOrdinal
      ) {
        sampleIndexMax--;
      }
    }
    await aggregateStatSamplesSlice(
      programAddress,
      runId,
      statName,
      statSamples,
      runCycleIndex,
      sampleIndexMin,
      sampleIndexMax,
    );
  }
  if (runId) {
    await utilsWritePointPlot(
      `${programAddress}`,
      runId,
      `history (${statName})`,
      statSamples.map((sample) => ({
        x: sample.time?.getTime() ?? NaN,
        y: sample.sumValue / sample.numValue,
      })),
      (x) => new Date(x).toISOString(),
    );
  }
}

async function aggregateStatSamplesSlice(
  programAddress: Pubkey,
  runId: string | undefined,
  statName: string,
  statSamples: Array<CoordinatorDataRunSample>,
  runCycleIndex: number,
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
  while (chunkSize * desiredSampleStepBucketCount < maxStep - minStep) {
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
  if (runId) {
    await utilsWritePointPlot(
      `${programAddress}`,
      runId,
      `run ${runCycleIndex + 1} (${statName})`,
      statSamples.slice(sampleIndexMin, sampleIndexMax + 1).map((sample) => ({
        x: sample.step,
        y: sample.sumValue / sample.numValue,
      })),
      (x) => `Step ${x}`,
    );
  }
}

const desiredSampleStepBucketCount = 1000;
