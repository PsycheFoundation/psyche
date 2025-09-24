import { TransactionSignature } from "@solana/web3.js";
import {
  jsonSchemaArray,
  jsonSchemaNumber,
  jsonSchemaNumberConst,
  jsonSchemaObject,
  jsonSchemaString,
  JsonValue,
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

const jsonSchemaV1 = jsonSchemaObject({
  version: jsonSchemaNumberConst(1),
  indexedOrderedChunks: jsonSchemaArray(
    jsonSchemaObject({
      orderingHigh: jsonSchemaString(),
      orderingLow: jsonSchemaString(),
      startedFrom: jsonSchemaString(),
      rewindedUntil: jsonSchemaString(),
      processedCounter: jsonSchemaNumber(),
    }),
  ),
});

export function indexingCheckpointToJson(
  checkpoint: IndexingCheckpoint,
): JsonValue {
  return jsonSchemaV1.guard({
    version: 1,
    indexedOrderedChunks: checkpoint.indexedOrderedChunks.map((chunk) => ({
      orderingHigh: String(chunk.orderingHigh),
      orderingLow: String(chunk.orderingLow),
      startedFrom: chunk.startedFrom,
      rewindedUntil: chunk.rewindedUntil,
      processedCounter: chunk.processedCounter,
    })),
  });
}

export function indexingCheckpointFromJson(
  jsonValue: JsonValue,
): IndexingCheckpoint {
  const jsonParsed = jsonSchemaV1.parse(jsonValue);
  return new IndexingCheckpoint(
    jsonParsed.indexedOrderedChunks.map((chunkValue) => {
      return {
        orderingHigh: BigInt(chunkValue.orderingHigh),
        orderingLow: BigInt(chunkValue.orderingLow),
        startedFrom: chunkValue.startedFrom,
        rewindedUntil: chunkValue.rewindedUntil,
        processedCounter: chunkValue.processedCounter,
      };
    }),
  );
}
