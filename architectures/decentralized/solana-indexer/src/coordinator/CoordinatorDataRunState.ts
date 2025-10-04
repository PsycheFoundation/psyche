import {
  jsonTypeArray,
  jsonTypeObject,
  jsonTypeString,
} from "solana-kiss-data";
import { utilsBigintStringJsonType } from "../utils";

export interface CoordinatorDataRunState {
  runId: string;
  name: string;
  description: string;
  status: string;
  epochClients: Array<{ signer: string; state: string }>;
  nonce: bigint;
}

export const coordinatorDataRunStateJsonType = jsonTypeObject({
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
  nonce: utilsBigintStringJsonType,
});
