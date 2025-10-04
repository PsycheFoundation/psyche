import {
  JsonType,
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  Signature,
} from "solana-kiss-data";
import { utilsOrderingJsonType } from "../utils";

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
        orderingHigh: utilsOrderingJsonType,
        orderingLow: utilsOrderingJsonType,
        startedFrom: jsonTypeString,
        rewindedUntil: jsonTypeString,
        processedCounter: jsonTypeNumber,
      }),
    ),
  });
