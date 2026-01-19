import {
  JsonCodec,
  jsonCodecArray,
  jsonCodecInteger,
  jsonCodecNumber,
  jsonCodecObject,
  jsonCodecSignature,
  Signature,
} from "solana-kiss";

export type IndexingCheckpointChunk = {
  newestTransactionId: Signature;
  oldestTransactionId: Signature;
  newestTransactionOrdinal: bigint;
  oldestTransactionOrdinal: bigint;
  transactionCounter: number;
};

export type IndexingCheckpoint = {
  orderedIndexedChunks: Array<IndexingCheckpointChunk>;
};

export const indexingCheckpointJsonCodec: JsonCodec<IndexingCheckpoint> =
  jsonCodecObject({
    orderedIndexedChunks: jsonCodecArray(
      jsonCodecObject({
        newestTransactionId: jsonCodecSignature,
        oldestTransactionId: jsonCodecSignature,
        newestTransactionOrdinal: jsonCodecInteger,
        oldestTransactionOrdinal: jsonCodecInteger,
        transactionCounter: jsonCodecNumber,
      }),
    ),
  });
