import { Application } from "express";
import {
  coordinatorRunAddressFromId,
  coordinatorRunAnalysisJsonCodec,
  coordinatorRunSummaryJsonCodec,
} from "psyche-indexer-codecs";
import { Pubkey, jsonCodecString, jsonEncoderArrayToArray } from "solana-kiss";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

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
    return res
      .status(200)
      .json(
        jsonEncoderArrayToArray(coordinatorRunSummaryJsonCodec.encoder)(
          runSummaries,
        ),
      );
  });
  expressApp.get(`/${programAddress}/coordinator/run/:runId`, (req, res) => {
    const runId = jsonCodecString.decoder(req.params.runId);
    const runAddress = coordinatorRunAddressFromId(programAddress, runId);
    const runAnalysis = dataStore.runAnalysisByAddress.get(runAddress);
    if (!runAnalysis) {
      return res.status(404).json({ error: "Run not found" });
    }
    return res
      .status(200)
      .json(coordinatorRunAnalysisJsonCodec.encoder(runAnalysis));
  });
}
