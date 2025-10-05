import {
  JsonType,
  jsonTypeArray,
  jsonTypeArrayToObject,
  jsonTypeDate,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
  Pubkey,
} from "solana-kiss-data";
import {
  CoordinatorDataRunState,
  coordinatorDataRunStateJsonType,
} from "./CoordinatorDataRunState";

export interface CoordinatorDataRunInfoWitness {
  processedTime: Date | undefined;
  ordering: bigint;
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
    string,
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
    ordering: bigint;
    instructionName: string;
    instructionAddresses: Map<string, Pubkey>;
    instructionPayload: JsonValue;
  }>;
}

const witnessJsonType: JsonType<CoordinatorDataRunInfoWitness> = jsonTypeObject(
  {
    processedTime: jsonTypeOptional(jsonTypeDate),
    ordering: jsonTypeInteger,
    metadata: jsonTypeObject({
      tokensPerSec: jsonTypeNumber,
      bandwidthPerSec: jsonTypeNumber,
      loss: jsonTypeNumber,
      step: jsonTypeNumber,
    }),
  },
);

export const coordinatorDataRunInfoJsonType: JsonType<CoordinatorDataRunInfo> =
  jsonTypeObject({
    accountState: jsonTypeOptional(coordinatorDataRunStateJsonType),
    accountUpdatedAt: jsonTypeOptional(jsonTypeDate),
    accountFetchedOrdering: jsonTypeInteger,
    accountRequestOrdering: jsonTypeInteger,
    witnessesPerUser: jsonTypeObjectToMap(
      jsonTypeObject({
        lastFew: jsonTypeArray(witnessJsonType),
        sampled: jsonTypeObject({
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
      jsonTypeObject({
        processedTime: jsonTypeOptional(jsonTypeDate),
        ordering: jsonTypeInteger,
        instructionName: jsonTypeString,
        instructionAddresses: jsonTypeObjectToMap(jsonTypeString),
        instructionPayload: jsonTypeValue,
      }),
    ),
  });
