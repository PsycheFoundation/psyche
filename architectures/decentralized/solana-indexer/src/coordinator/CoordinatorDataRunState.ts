import {
  JsonType,
  jsonTypeArray,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypeString,
} from "solana-kiss-data";

export interface CoordinatorDataRunState {
  runId: string;
  name: string;
  description: string;
  status: string;
  epochClients: Array<{ signer: string; state: string }>;
  nonce: bigint;
}

export const coordinatorDataRunStateJsonType: JsonType<CoordinatorDataRunState> =
  jsonTypeObject({
    runId: jsonTypeString,
    name: jsonTypeString,
    description: jsonTypeString,
    status: jsonTypeString,
    epochClients: jsonTypeArray(
      jsonTypeObject({
        signer: jsonTypeString,
        state: jsonTypeString,
      }),
    ),
    nonce: jsonTypeInteger,
  });
