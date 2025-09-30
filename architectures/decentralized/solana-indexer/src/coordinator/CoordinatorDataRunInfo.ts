import {
  JsonType,
  jsonTypeArray,
  jsonTypeArrayToVariant,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
  jsonTypeStringToBigint,
} from "../json";
import { jsonTypeObjectSnakeCase } from "../utils";
import {
  CoordinatorDataRunState,
  coordinatorDataRunStateJsonType,
} from "./CoordinatorDataRunState";

export interface CoordinatorDataRunInfoWitness {
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
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
  witnessesPerUser: Map<
    string,
    {
      lastFew: Array<CoordinatorDataRunInfoWitness>;
      sampled: {
        rate: number;
        data: Array<CoordinatorDataRunInfoWitness>;
      };
    }
  >;
}

const witnessJsonTypeV1: JsonType<CoordinatorDataRunInfoWitness> =
  jsonTypeObjectSnakeCase({
    ordering: jsonTypeStringToBigint(),
    metadata: jsonTypeObjectSnakeCase({
      tokensPerSec: jsonTypeNumber(),
      bandwidthPerSec: jsonTypeNumber(),
      loss: jsonTypeNumber(),
      step: jsonTypeNumber(),
    }),
  });

const jsonTypeV1 = jsonTypeArrayToVariant(
  "RunInfo(v1)",
  jsonTypeObject({
    accountState: jsonTypeOptional(coordinatorDataRunStateJsonType),
    accountFetchedOrdering: jsonTypeStringToBigint(),
    accountRequestOrdering: jsonTypeStringToBigint(),
    witnessesPerUser: jsonTypeObjectToMap(
      jsonTypeObjectSnakeCase({
        lastFew: jsonTypeArray(witnessJsonTypeV1),
        sampled: jsonTypeObjectSnakeCase({
          rate: jsonTypeNumber(),
          data: jsonTypeArray(witnessJsonTypeV1),
        }),
      }),
    ),
  }),
);

export const coordinatorDataRunInfoJsonType: JsonType<CoordinatorDataRunInfo> =
  jsonTypeV1;
