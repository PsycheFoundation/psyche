import {
  jsonTypeArray,
  jsonTypeArrayToVariant,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeStringToBigint,
} from "../json";

export interface CoordinatorDataRunState {
  runId: string;
  name: string;
  description: string;
  status: string;
  epochClients: Array<{ signer: string; state: string }>;
  nonce: bigint;
}

const jsonTypeV1 = jsonTypeArrayToVariant(
  "RunState(v1)",
  jsonTypeObject({
    runId: jsonTypeString(),
    name: jsonTypeString(),
    description: jsonTypeString(),
    status: jsonTypeString(),
    epochClients: jsonTypeArray(
      jsonTypeObject({
        signer: jsonTypeString(),
        state: jsonTypeString(),
      }),
    ),
    nonce: jsonTypeStringToBigint(),
  }),
);

export const coordinatorDataRunStateJsonType = jsonTypeV1;
