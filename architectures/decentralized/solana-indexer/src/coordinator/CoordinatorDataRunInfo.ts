import {
  JsonType,
  jsonTypeArray,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeOptional,
} from "solana-kiss-data";
import { utilsBigintStringJsonType } from "../utils";
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

const witnessJsonType: JsonType<CoordinatorDataRunInfoWitness> = jsonTypeObject(
  {
    ordering: utilsBigintStringJsonType,
    metadata: jsonTypeObject({
      tokensPerSec: jsonTypeNumber,
      bandwidthPerSec: jsonTypeNumber,
      loss: jsonTypeNumber,
      step: jsonTypeNumber,
    }),
  },
);

export const coordinatorDataRunInfoJsonType = jsonTypeObject({
  accountState: jsonTypeOptional(coordinatorDataRunStateJsonType),
  accountFetchedOrdering: utilsBigintStringJsonType,
  accountRequestOrdering: utilsBigintStringJsonType,
  witnessesPerUser: jsonTypeObjectToMap(
    jsonTypeObject({
      lastFew: jsonTypeArray(witnessJsonType),
      sampled: jsonTypeObject({
        rate: jsonTypeNumber,
        data: jsonTypeArray(witnessJsonType),
      }),
    }),
  ),
});
