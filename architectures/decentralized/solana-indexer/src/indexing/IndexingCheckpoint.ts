import { TransactionSignature } from "@solana/web3.js";
import {
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
  jsonTypeWrap,
} from "../jsonType";

export type IndexingCheckpointChunk = {
  orderingHigh: bigint;
  orderingLow: bigint;
  startedFrom: TransactionSignature;
  rewindedUntil: TransactionSignature;
  processedCounter: number;
};

export class IndexingCheckpoint {
  public readonly indexedOrderedChunks: ReadonlyArray<
    Readonly<IndexingCheckpointChunk>
  >;

  constructor(
    indexedOrderedChunks: ReadonlyArray<Readonly<IndexingCheckpointChunk>>,
  ) {
    this.indexedOrderedChunks = indexedOrderedChunks;
  }
}

const indexingCheckpointJsonTypeV1 = jsonTypeWrap(
  jsonTypeObject({
    indexedOrderedChunks: jsonTypeArray(
      jsonTypeObject({
        orderingHigh: jsonTypeStringToBigint(),
        orderingLow: jsonTypeStringToBigint(),
        startedFrom: jsonTypeString(),
        rewindedUntil: jsonTypeString(),
        processedCounter: jsonTypeNumber(),
      }),
    ),
  }),
  (decoded) => new IndexingCheckpoint(decoded.indexedOrderedChunks),
  (encoded) => ({ indexedOrderedChunks: encoded.indexedOrderedChunks }),
);

// TODO - support versionning ?
export const indexingCheckpointJsonType = jsonTypeWrap(
  indexingCheckpointJsonTypeV1,
  (decoded) => decoded,
  (encoded) => encoded,
);
