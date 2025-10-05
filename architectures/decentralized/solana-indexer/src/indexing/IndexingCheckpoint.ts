import {
  JsonType,
  jsonTypeArray,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  Signature,
} from "solana-kiss-data";

export type IndexingCheckpointChunk = {
  orderingHigh: bigint;
  orderingLow: bigint;
  startedFrom: Signature;
  rewindedUntil: Signature;
  processedCounter: number;
};

export type IndexingCheckpoint = {
  indexedChunks: Array<IndexingCheckpointChunk>;
};

export const indexingCheckpointJsonType: JsonType<IndexingCheckpoint> =
  jsonTypeObject({
    indexedChunks: jsonTypeArray(
      jsonTypeObject({
        orderingHigh: jsonTypeInteger,
        orderingLow: jsonTypeInteger,
        startedFrom: jsonTypeString,
        rewindedUntil: jsonTypeString,
        processedCounter: jsonTypeNumber,
      }),
    ),
  });
