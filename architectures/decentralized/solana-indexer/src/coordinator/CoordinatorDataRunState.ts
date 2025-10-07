import {
  JsonType,
  jsonTypeArray,
  jsonTypeInteger,
  jsonTypeObject,
  jsonTypePubkey,
  jsonTypeString,
  Pubkey,
} from "solana-kiss";

export interface CoordinatorDataRunState {
  runId: string;
  name: string;
  description: string;
  status: string;
  epochClients: Array<{ signer: Pubkey; state: string }>;
  nonce: bigint;
}

export const coordinatorDataRunStateJsonType: JsonType<CoordinatorDataRunState> =
  jsonTypeObject((key) => key, {
    runId: jsonTypeString,
    name: jsonTypeString,
    description: jsonTypeString,
    status: jsonTypeString,
    epochClients: jsonTypeArray(
      jsonTypeObject((key) => key, {
        signer: jsonTypePubkey,
        state: jsonTypeString,
      }),
    ),
    nonce: jsonTypeInteger,
  });
