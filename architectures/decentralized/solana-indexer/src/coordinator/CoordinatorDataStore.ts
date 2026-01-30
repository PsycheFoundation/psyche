import {
  CoordinatorRunAnalysis,
  coordinatorRunAnalysisJsonCodec,
} from "psyche-indexer-codecs";
import {
  JsonCodec,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecWrapped,
  Pubkey,
} from "solana-kiss";
import { jsonCodecObjectToMapByPubkey } from "../json";

export class CoordinatorDataStore {
  public programAddress: Pubkey;
  public runAnalysisByAddress: Map<Pubkey, CoordinatorRunAnalysis>;

  constructor(
    programAddress: Pubkey,
    runAnalysisByAddress: Map<Pubkey, CoordinatorRunAnalysis>,
  ) {
    this.programAddress = programAddress;
    this.runAnalysisByAddress = runAnalysisByAddress;
  }

  public getRunAnalysis(runAddress: Pubkey): CoordinatorRunAnalysis {
    let runAnalysis = this.runAnalysisByAddress.get(runAddress);
    if (runAnalysis === undefined) {
      runAnalysis = {
        latestKnownChangeOrdinal: 0n,
        latestUpdateFetchOrdinal: 0n,
        latestOnchainSnapshot: null,
        lastWitnessByUser: new Map(),
        samplesByStatName: new Map(),
        adminHistory: [],
        joinHistory: [],
        checkpointHistory: [],
        finishesOrdinals: [],
      };
      this.runAnalysisByAddress.set(runAddress, runAnalysis);
    }
    return runAnalysis;
  }
}

export const coordinatorDataStoreJsonCodec: JsonCodec<CoordinatorDataStore> =
  jsonCodecWrapped(
    jsonCodecObjectToObject({
      programAddress: jsonCodecPubkey,
      runAnalysisByAddress: jsonCodecObjectToMapByPubkey(
        coordinatorRunAnalysisJsonCodec,
      ),
    }),
    {
      decoder: (encoded) =>
        new CoordinatorDataStore(
          encoded.programAddress,
          encoded.runAnalysisByAddress,
        ),
      encoder: (decoded) => ({
        programAddress: decoded.programAddress,
        runAnalysisByAddress: decoded.runAnalysisByAddress,
      }),
    },
  );
