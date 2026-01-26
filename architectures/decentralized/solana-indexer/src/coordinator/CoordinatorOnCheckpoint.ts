import { Solana } from "solana-kiss";
import { utilRunInParallel } from "../utils";
import { CoordinatorDataStore } from "./CoordinatorDataStore";
import { coordinatorRunOnchainFetch } from "./CoordinatorRunOnchainFetch";
import { coordinatorRunSampleAggregate } from "./CoordinatorRunSampleAggregate";

export async function coordinatorOnCheckpoint(
  solana: Solana,
  dataStore: CoordinatorDataStore,
) {
  const tasks = await utilRunInParallel(
    dataStore.runAnalysisByAddress.entries(),
    async ([runAddress, runAnalysis]) => {
      await coordinatorRunOnchainFetch(solana, runAddress, runAnalysis);
      await coordinatorRunSampleAggregate(
        dataStore.programAddress,
        runAnalysis,
      );
    },
  );
  for (const task of tasks) {
    if (task.result.error) {
      console.error(
        "Failed to process run checkpoint",
        task.input[0],
        task.result.error,
      );
    }
  }
}
