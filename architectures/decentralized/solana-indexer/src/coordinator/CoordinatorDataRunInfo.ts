import {
  JsonType,
  jsonTypeArray,
  jsonTypeArrayToObject,
  jsonTypeBoolean,
  jsonTypeDateTime,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeOptional,
  jsonTypePubkey,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
  Pubkey,
} from "solana-kiss";
import {
  utilsObjectToPubkeyMapJsonType,
  utilsObjectToStringMapJsonType,
} from "../utils";
import {
  CoordinatorDataRunState,
  coordinatorDataRunStateJsonType,
} from "./CoordinatorDataRunState";

export interface CoordinatorDataRunInfoWitness {
  processedTime: Date | undefined;
  ordering: bigint;
  proof: {
    position: bigint;
    index: bigint;
    witness: boolean;
  };
  metadata: {
    tokensPerSec: number;
    bandwidthPerSec: number;
    loss: number;
    step: number;
  };
}

export interface CoordinatorDataRunInfo {
  accountState: CoordinatorDataRunState | undefined;
  accountUpdatedAt: Date | undefined;
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
  witnessesPerUser: Map<
    Pubkey,
    {
      lastFew: Array<CoordinatorDataRunInfoWitness>;
      sampled: {
        rate: number;
        data: Array<{
          selector: number;
          witness: CoordinatorDataRunInfoWitness;
        }>;
      };
    }
  >;
  adminHistory: Array<{
    processedTime: Date | undefined;
    signerAddress: Pubkey;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
    ordering: bigint;
  }>;
}

const witnessJsonType: JsonType<CoordinatorDataRunInfoWitness> = jsonTypeObject(
  (key) => key,
  {
    processedTime: jsonTypeOptional(jsonTypeDateTime),
    ordering: jsonTypeInteger,
    proof: jsonTypeObject((key) => key, {
      position: jsonTypeInteger,
      index: jsonTypeInteger,
      witness: jsonTypeBoolean,
    }),
    metadata: jsonTypeObject((key) => key, {
      tokensPerSec: jsonTypeNumber,
      bandwidthPerSec: jsonTypeNumber,
      loss: jsonTypeNumber,
      step: jsonTypeNumber,
    }),
  },
);

export const coordinatorDataRunInfoJsonType: JsonType<CoordinatorDataRunInfo> =
  jsonTypeObject((key) => key, {
    accountState: jsonTypeOptional(coordinatorDataRunStateJsonType),
    accountUpdatedAt: jsonTypeOptional(jsonTypeDateTime),
    accountFetchedOrdering: jsonTypeInteger,
    accountRequestOrdering: jsonTypeInteger,
    witnessesPerUser: utilsObjectToPubkeyMapJsonType(
      jsonTypeObject((key) => key, {
        lastFew: jsonTypeArray(witnessJsonType),
        sampled: jsonTypeObject((key) => key, {
          rate: jsonTypeNumber,
          data: jsonTypeArray(
            jsonTypeArrayToObject({
              selector: jsonTypeNumber,
              witness: witnessJsonType,
            }),
          ),
        }),
      }),
    ),
    adminHistory: jsonTypeArray(
      jsonTypeObject((key) => key, {
        processedTime: jsonTypeOptional(jsonTypeDateTime),
        signerAddress: jsonTypePubkey,
        instructionName: jsonTypeString,
        instructionAddresses: utilsObjectToStringMapJsonType(jsonTypePubkey),
        instructionPayload: jsonTypeValue,
        ordering: jsonTypeInteger,
      }),
    ),
  });
