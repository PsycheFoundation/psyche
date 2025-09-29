import { TransactionSignature } from "@solana/web3.js";
import {
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../json";

export type IndexingCheckpointChunk = {
  orderingHigh: bigint;
  orderingLow: bigint;
  startedFrom: TransactionSignature;
  rewindedUntil: TransactionSignature;
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
      startedFrom: jsonTypeString(),
      rewindedUntil: jsonTypeString(),
      processedCounter: jsonTypeNumber(),
    }),
  ),
});
