import {
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  Signature,
} from "solana-kiss-data";
import { utilsBigintStringJsonType } from "../utils";

export type IndexingCheckpointChunk = {
  orderingHigh: bigint;
  orderingLow: bigint;
  startedFrom: Signature;
  rewindedUntil: Signature;
  processedCounter: number;
};

export type IndexingCheckpoint = {
  indexedChunks: ReadonlyArray<Readonly<IndexingCheckpointChunk>>;
};

export const indexingCheckpointJsonType = jsonTypeObject({
  indexedChunks: jsonTypeArray(
    jsonTypeObject({
      orderingHigh: utilsBigintStringJsonType,
      orderingLow: utilsBigintStringJsonType,
      startedFrom: jsonTypeString,
      rewindedUntil: jsonTypeString,
      processedCounter: jsonTypeNumber,
    }),
  ),
});
