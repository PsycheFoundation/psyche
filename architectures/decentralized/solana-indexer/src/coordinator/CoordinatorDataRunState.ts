import {
  JsonCodec,
  jsonCodecArray,
  jsonCodecInteger,
  jsonCodecNumber,
  jsonCodecObject,
  jsonCodecPubkey,
  jsonCodecRaw,
  jsonCodecString,
  JsonValue,
  Pubkey,
} from "solana-kiss";

export interface CoordinatorDataRunState {
  runId: string;
  coordinatorInstanceAddress: Pubkey;
  coordinatorAccountAddress: Pubkey;
  mainAuthority: Pubkey;
  joinAuthority: Pubkey;
  name: string;
  description: string;
  status: string;
  model: JsonValue;
  numParameters: bigint;
  joinedClients: Array<{ signer: Pubkey; earned: bigint; slashed: bigint }>;
  epochClients: Array<{ signer: Pubkey; state: string }>;
  epochRates: {
    current: { earningRate: bigint; slashingRate: bigint };
    future: { earningRate: bigint; slashingRate: bigint };
  };
  progress: {
    epoch: number;
    step: number;
  };
  nonce: bigint;
}

export const coordinatorDataRunStateJsonCodec: JsonCodec<CoordinatorDataRunState> =
  jsonCodecObject({
    runId: jsonCodecString,
    coordinatorInstanceAddress: jsonCodecPubkey,
    coordinatorAccountAddress: jsonCodecPubkey,
    mainAuthority: jsonCodecPubkey,
    joinAuthority: jsonCodecPubkey,
    name: jsonCodecString,
    description: jsonCodecString,
    status: jsonCodecString,
    model: jsonCodecRaw,
    numParameters: jsonCodecInteger,
    joinedClients: jsonCodecArray(
      jsonCodecObject({
        signer: jsonCodecPubkey,
        earned: jsonCodecInteger,
        slashed: jsonCodecInteger,
      }),
    ),
    epochClients: jsonCodecArray(
      jsonCodecObject({ signer: jsonCodecPubkey, state: jsonCodecString }),
    ),
    epochRates: jsonCodecObject({
      current: jsonCodecObject({
        earningRate: jsonCodecInteger,
        slashingRate: jsonCodecInteger,
      }),
      future: jsonCodecObject({
        earningRate: jsonCodecInteger,
        slashingRate: jsonCodecInteger,
      }),
    }),
    progress: jsonCodecObject({
      epoch: jsonCodecNumber,
      step: jsonCodecNumber,
    }),
    nonce: jsonCodecInteger,
  });
