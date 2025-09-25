import { TransactionSignature } from "@solana/web3.js";
import { JsonValue } from "../json";
import {
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
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

const jsonTypeV1 = jsonTypeObject({
  version: jsonTypeNumber(),
  indexedOrderedChunks: jsonTypeArray(
    jsonTypeObject({
      orderingHigh: jsonTypeStringToBigint(),
      orderingLow: jsonTypeStringToBigint(),
      startedFrom: jsonTypeString(),
      rewindedUntil: jsonTypeString(),
      processedCounter: jsonTypeNumber(),
    }),
  ),
});

export function indexingCheckpointToJson(
  checkpoint: IndexingCheckpoint,
): JsonValue {
  return jsonTypeV1.encode({
    version: 1,
    indexedOrderedChunks: checkpoint.indexedOrderedChunks,
  });
}

export function indexingCheckpointFromJson(
  jsonValue: JsonValue,
): IndexingCheckpoint {
  const decoded = jsonTypeV1.decode(jsonValue);
  if (decoded.version !== 1) {
    throw new Error(`Unsupported indexing checkpoint version`);
  }
  return new IndexingCheckpoint(decoded.indexedOrderedChunks);
}
