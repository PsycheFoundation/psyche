import {
  jsonCodecArrayToArray,
  jsonCodecArrayToObject,
  jsonCodecBigInt,
  JsonCodecContent,
  jsonCodecDateTime,
  jsonCodecNullable,
  jsonCodecNumber,
  jsonCodecObjectToMap,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecString,
  jsonCodecValue,
} from "solana-kiss";
import { indexerInstructionJsonCodec } from "../indexer/IndexerTypes";
import { jsonCodecObjectToMapByPubkey } from "../json";

export type CoordinatorDataRunOnchain = JsonCodecContent<
  typeof coordinatorDataRunOnchainJsonCodec
>;

export type CoordinatorDataRunSample = JsonCodecContent<
  typeof coordinatorDataRunSampleJsonCodec
>;

export type CoordinatorDataRunAnalysis = JsonCodecContent<
  typeof coordinatorDataRunAnalysisJsonCodec
>;

export const coordinatorDataRunOnchainJsonCodec = jsonCodecObjectToObject({
  runId: jsonCodecString,
  mainAuthority: jsonCodecPubkey,
  joinAuthority: jsonCodecPubkey,
  name: jsonCodecString,
  description: jsonCodecString,
  status: jsonCodecString,
  numParameters: jsonCodecBigInt,
  joinedClients: jsonCodecArrayToArray(
    jsonCodecObjectToObject({
      signer: jsonCodecPubkey,
      earned: jsonCodecBigInt,
      slashed: jsonCodecBigInt,
    }),
  ),
  epochClients: jsonCodecArrayToArray(
    jsonCodecObjectToObject({
      signer: jsonCodecPubkey,
      state: jsonCodecString,
    }),
  ),
  /*
  epochRates: jsonCodecObjectToObject({
    current: jsonCodecObjectToObject({
      earningRate: jsonCodecBigInt,
      slashingRate: jsonCodecBigInt,
    }),
    future: jsonCodecObjectToObject({
      earningRate: jsonCodecBigInt,
      slashingRate: jsonCodecBigInt,
    }),
  }),
  */
  progress: jsonCodecObjectToObject({
    epoch: jsonCodecNumber,
    step: jsonCodecNumber,
  }),
});

const coordinatorDataRunSampleJsonCodec = jsonCodecArrayToObject({
  maxOrdinal: jsonCodecBigInt,
  step: jsonCodecNumber,
  sumValue: jsonCodecNumber,
  numValue: jsonCodecNumber,
  time: jsonCodecNullable(jsonCodecDateTime),
});

export const coordinatorDataRunAnalysisJsonCodec = jsonCodecObjectToObject({
  latestKnownChangeOrdinal: jsonCodecBigInt,
  latestUpdateFetchOrdinal: jsonCodecBigInt,
  latestOnchainSnapshot: jsonCodecNullable(
    jsonCodecObjectToObject({
      parsed: coordinatorDataRunOnchainJsonCodec,
      native: jsonCodecObjectToObject({
        coordinatorInstance: jsonCodecValue,
        coordinatorAccount: jsonCodecValue,
      }),
      updatedAt: jsonCodecDateTime,
    }),
  ),
  lastWitnessByUser: jsonCodecObjectToMapByPubkey(
    jsonCodecObjectToObject({
      ordinal: jsonCodecBigInt,
      step: jsonCodecNumber,
    }),
  ),
  samplesByStatName: jsonCodecObjectToMap({
    keyCodec: {
      decoder: (name) => name,
      encoder: (name) => name,
    },
    valueCodec: jsonCodecArrayToArray(coordinatorDataRunSampleJsonCodec),
  }),
  adminHistory: jsonCodecArrayToArray(indexerInstructionJsonCodec),
  joinHistory: jsonCodecArrayToArray(
    jsonCodecObjectToObject({
      blockTime: jsonCodecNullable(jsonCodecDateTime),
      instructionOrdinal: jsonCodecBigInt,
      user: jsonCodecPubkey,
      p2pIdentity: jsonCodecPubkey,
    }),
  ),
  checkpointHistory: jsonCodecArrayToArray(
    jsonCodecObjectToObject({
      blockTime: jsonCodecNullable(jsonCodecDateTime),
      instructionOrdinal: jsonCodecBigInt,
      user: jsonCodecPubkey,
      repo: jsonCodecObjectToObject({
        repoId: jsonCodecString,
        revision: jsonCodecNullable(jsonCodecString),
      }),
    }),
  ),
  finishesOrdinals: jsonCodecArrayToArray(jsonCodecBigInt),
});
