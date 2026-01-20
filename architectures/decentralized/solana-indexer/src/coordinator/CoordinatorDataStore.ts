import {
  JsonCodec,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecWrapped,
  Pubkey,
  pubkeyFindPdaAddress,
  utf8Encode,
} from "solana-kiss";
import { jsonCodecObjectToMapByPubkey } from "../json";
import {
  CoordinatorDataRunAnalysis,
  coordinatorDataRunAnalysisJsonCodec,
} from "./CoordinatorDataTypes";

export class CoordinatorDataStore {
  public programAddress: Pubkey;
  public runAnalysisByAddress: Map<Pubkey, CoordinatorDataRunAnalysis>;

  constructor(
    programAddress: Pubkey,
    runAnalysisByAddress: Map<Pubkey, CoordinatorDataRunAnalysis>,
  ) {
    this.programAddress = programAddress;
    this.runAnalysisByAddress = runAnalysisByAddress;
  }

  public getRunAddress(runId: string): Pubkey {
    const runIdSeed = new Uint8Array(32);
    runIdSeed.set(utf8Encode(runId).slice(0, 32));
    return pubkeyFindPdaAddress(this.programAddress, [
      utf8Encode("coordinator"),
      runIdSeed,
    ]);
  }

  public getRunInfo(runAddress: Pubkey): CoordinatorDataRunAnalysis {
    let runAnalysis = this.runAnalysisByAddress.get(runAddress);
    if (runAnalysis === undefined) {
      runAnalysis = {
        latestKnownChangeOrdinal: 0n,
        latestUpdateFetchOrdinal: 0n,
        latestOnchainSnapshot: null,
        lastWitnessByUser: new Map(),
        samplesByStatName: new Map(),
        finishesOrdinals: [],
        importantHistory: [],
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
        coordinatorDataRunAnalysisJsonCodec,
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
