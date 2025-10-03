import {
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  Signature,
} from "solana-kiss-data";
import { jsonTypeStringToBigint } from "../utils";

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
      orderingHigh: jsonTypeStringToBigint(),
      orderingLow: jsonTypeStringToBigint(),
      startedFrom: jsonTypeString,
      rewindedUntil: jsonTypeString,
      processedCounter: jsonTypeNumber,
    }),
  ),
});
