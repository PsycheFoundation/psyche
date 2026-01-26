import {
  jsonCodecBigInt,
  JsonCodecContent,
  jsonCodecDateTime,
  jsonCodecNullable,
  jsonCodecObjectToObject,
  jsonCodecObjectToRecord,
  jsonCodecPubkey,
  jsonCodecString,
  jsonCodecValue,
} from "solana-kiss";

export type IndexerInstruction = JsonCodecContent<
  typeof indexerInstructionJsonCodec
>;

export const indexerInstructionJsonCodec = jsonCodecObjectToObject({
  blockTime: jsonCodecNullable(jsonCodecDateTime),
  instructionOrdinal: jsonCodecBigInt,
  instructionName: jsonCodecString,
  instructionAddresses: jsonCodecObjectToRecord(jsonCodecPubkey),
  instructionPayload: jsonCodecValue,
});
