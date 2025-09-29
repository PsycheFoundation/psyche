import {
  jsonTypeArrayToVariant,
  jsonTypeObject,
  jsonTypeOptional,
  jsonTypeStringToBigint,
} from "../json";
import {
  CoordinatorDataRunState,
  coordinatorDataRunStateJsonType,
} from "./CoordinatorDataRunState";

export interface CoordinatorDataRunInfo {
  accountState: CoordinatorDataRunState | undefined;
  accountFetchedOrdering: bigint;
  accountRequestOrdering: bigint;
}

const jsonTypeV1 = jsonTypeArrayToVariant(
  "RunInfo(v1)",
  jsonTypeObject({
    accountState: jsonTypeOptional(coordinatorDataRunStateJsonType),
    accountFetchedOrdering: jsonTypeStringToBigint(),
    accountRequestOrdering: jsonTypeStringToBigint(),
  }),
);

export const coordinatorDataRunInfoJsonType = jsonTypeV1;
