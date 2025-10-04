import {
  JsonType,
  jsonTypeArray,
  jsonTypeArrayToObject,
  jsonTypeDate,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
} from "solana-kiss-data";
import { utilsOrderingJsonType } from "../utils";
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
}

const witnessJsonType: JsonType<CoordinatorDataRunInfoWitness> = jsonTypeObject(
  {
    processedTime: jsonTypeOptional(jsonTypeDate),
    ordering: utilsOrderingJsonType,
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
    accountFetchedOrdering: utilsOrderingJsonType,
    accountRequestOrdering: utilsOrderingJsonType,
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
  });
