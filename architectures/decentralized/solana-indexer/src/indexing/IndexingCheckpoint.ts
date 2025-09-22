import { TransactionSignature } from "@solana/web3.js";

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

  public toJson(): any {
    return {
      indexedOrderedChunks: this.indexedOrderedChunks.map((chunk) => ({
        orderingHigh: String(chunk.orderingHigh),
        orderingLow: String(chunk.orderingLow),
        startedFrom: chunk.startedFrom,
        rewindedUntil: chunk.rewindedUntil,
        processedCounter: chunk.processedCounter,
      })),
    };
  }

  public static fromJson(obj: any): IndexingCheckpoint {
    const indexedOrderedChunks = obj?.["indexedOrderedChunks"];
    if (!Array.isArray(indexedOrderedChunks)) {
      throw new Error("Invalid indexedOrderedChunks");
    }
    for (const chunk of indexedOrderedChunks) {
      if (
        typeof chunk?.["orderingHigh"] !== "string" ||
        typeof chunk?.["orderingLow"] !== "string" ||
        typeof chunk?.["startedFrom"] !== "string" ||
        typeof chunk?.["rewindedUntil"] !== "string" ||
        typeof chunk?.["processedCounter"] !== "number"
      ) {
        throw new Error("Invalid chunk");
      }
    }
    return new IndexingCheckpoint(
      indexedOrderedChunks.map((chunk: any) => ({
        orderingHigh: BigInt(chunk["orderingHigh"]),
        orderingLow: BigInt(chunk["orderingLow"]),
        startedFrom: chunk["startedFrom"],
        rewindedUntil: chunk["rewindedUntil"],
        processedCounter: chunk["processedCounter"],
      })),
    );
  }
}
