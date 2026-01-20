import {
  IdlInstructionAddresses,
  jsonCodecBigInt,
  jsonCodecDateTime,
  jsonCodecNullable,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecString,
  jsonCodecValue,
  JsonValue,
} from "solana-kiss";
import { jsonCodecObjectToRecord } from "../json";

export interface IndexerInstruction {
  blockTime: Date | null;
  instructionOrdinal: bigint;
  instructionName: string;
  instructionAddresses: IdlInstructionAddresses;
  instructionPayload: JsonValue;
}

export const indexerInstructionJsonCodec = jsonCodecObjectToObject({
  blockTime: jsonCodecNullable(jsonCodecDateTime),
  instructionOrdinal: jsonCodecBigInt,
  instructionName: jsonCodecString,
  instructionAddresses: jsonCodecObjectToRecord(jsonCodecPubkey),
  instructionPayload: jsonCodecValue,
});
