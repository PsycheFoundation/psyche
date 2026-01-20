import {
  jsonCodecArrayToArray,
  jsonCodecBigInt,
  JsonCodecContent,
  jsonCodecNumber,
  jsonCodecObjectToObject,
  jsonCodecSignature,
} from "solana-kiss";

export type CrawlerTransaction = JsonCodecContent<
  typeof crawlerTransactionJsonCodec
>;

export type CrawlerCheckpoint = JsonCodecContent<
  typeof crawlerCheckpointJsonCodec
>;

const crawlerTransactionJsonCodec = jsonCodecObjectToObject({
  transactionHandle: jsonCodecSignature,
  transactionOrdinal: jsonCodecBigInt,
});

export const crawlerCheckpointJsonCodec = jsonCodecArrayToArray(
  jsonCodecObjectToObject({
    newestTransaction: crawlerTransactionJsonCodec,
    oldestTransaction: crawlerTransactionJsonCodec,
    transactionCounter: jsonCodecNumber,
  }),
);
