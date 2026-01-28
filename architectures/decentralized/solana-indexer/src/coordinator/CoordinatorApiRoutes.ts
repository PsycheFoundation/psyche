import { Application } from "express";
import {
  Pubkey,
  jsonCodecArrayToArray,
  jsonCodecObjectToObject,
  jsonCodecPubkey,
  jsonCodecString,
  pubkeyFindPdaAddress,
  utf8Encode,
} from "solana-kiss";
import { CoordinatorDataStore } from "./CoordinatorDataStore";
import {
  coordinatorDataRunAnalysisJsonCodec,
  coordinatorDataRunOnchainJsonCodec,
} from "./CoordinatorDataTypes";

export async function coordinatorApiRoutes(
  programAddress: Pubkey,
  expressApp: Application,
  dataStore: CoordinatorDataStore,
) {
  expressApp.get(`/${programAddress}/coordinator/summaries`, (_, res) => {
    const runSummaries = [];
    for (const [runAddress, runAnalysis] of dataStore.runAnalysisByAddress) {
      const runState = runAnalysis?.latestOnchainSnapshot?.parsed;
      if (runState === undefined) {
        continue;
      }
      runSummaries.push({ address: runAddress, state: runState });
    }
    return res.status(200).json(runSummariesJsonCodec.encoder(runSummaries));
  });
  expressApp.get(`/${programAddress}/coordinator/run/:runId`, (req, res) => {
    const runId = jsonCodecString.decoder(req.params.runId);
    const runAddress = getRunAddress(programAddress, runId);
    const runAnalysis = dataStore.runAnalysisByAddress.get(runAddress);
    if (!runAnalysis) {
      return res.status(404).json({ error: "Run not found" });
    }
    return res
      .status(200)
      .json(coordinatorDataRunAnalysisJsonCodec.encoder(runAnalysis));
  });
}

function getRunAddress(programAddress: Pubkey, runId: string): Pubkey {
  return pubkeyFindPdaAddress(programAddress, [
    utf8Encode("coordinator"),
    utf8Encode(runId).slice(0, 32),
  ]);
}

const runSummariesJsonCodec = jsonCodecArrayToArray(
  jsonCodecObjectToObject({
    address: jsonCodecPubkey,
    state: coordinatorDataRunOnchainJsonCodec,
  }),
);
