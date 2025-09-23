import { TransactionSignature } from "@solana/web3.js";
import {
  jsonSchemaArray,
  jsonSchemaNumber,
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
  public readonly indexedOrderedChunks: Readonly<
    Array<IndexingCheckpointChunk>
  >;

  constructor(indexedOrderedChunks: Readonly<Array<IndexingCheckpointChunk>>) {
    this.indexedOrderedChunks = indexedOrderedChunks;
  }
}

const indexingCheckpointJsonSchema = jsonSchemaObject({
  version: jsonSchemaNumber(),
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
  return indexingCheckpointJsonSchema.guard({
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
  const jsonParsed = indexingCheckpointJsonSchema.parse(jsonValue);
  if (jsonParsed.version !== 1) {
    throw new Error("Unsupported version");
  }
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
