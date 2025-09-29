import { TransactionSignature } from "@solana/web3.js";
import {
  jsonTypeArray,
  jsonTypeMapped,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToVariant,
  jsonTypeString,
  jsonTypeStringToBigint,
  jsonTypeWithDecodeFallbacks,
} from "../json";

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

const jsonTypeV1 = jsonTypeObjectToVariant(
  "IndexingCheckpoint:v1",
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
);

export const indexingCheckpointJsonType = jsonTypeMapped(
  jsonTypeWithDecodeFallbacks(jsonTypeV1, []),
  {
    map: (unmapped) => new IndexingCheckpoint(unmapped.indexedOrderedChunks),
    unmap: (mapped) => ({ indexedOrderedChunks: mapped.indexedOrderedChunks }),
  },
);
