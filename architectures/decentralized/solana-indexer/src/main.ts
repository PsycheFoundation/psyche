import express from "express";
import {
  pubkeyFromBase58,
  rpcHttpFromUrl,
  rpcHttpWithMaxConcurrentRequests,
  rpcHttpWithRetryOnError,
  Solana,
  timeoutMs,
} from "solana-kiss";
import { coordinatorService } from "./coordinator/CoordinatorService";
import { miningPoolService } from "./mining-pool/MiningPoolService";
import { utilsGetEnv } from "./utils";

// TODO - dont use solana directly to leave capability of loading idls manually
function makeSolanaEndpoint(url: string) {
  return new Solana(
    rpcHttpWithRetryOnError(
      rpcHttpWithMaxConcurrentRequests(
        rpcHttpFromUrl(url, { commitment: "confirmed" }),
        100,
      ),
      async (context) => {
        if (context.totalDurationMs >= 60 * 60 * 1000) {
          console.error("Giving up retries after 1 hour", context);
          return false;
        }
        await timeoutMs(context.retriedCounter * 1000);
        return true;
      },
    ),
  );
}

async function main() {
  const expressApp = express();
  const httpApiPort = process.env["PORT"] ?? 3000;
  expressApp.listen(httpApiPort, (error) => {
    if (error) {
      console.error("Error starting server:", error);
    } else {
      console.log(`Listening on port ${httpApiPort}`);
    }
  });
  miningPoolService(
    makeSolanaEndpoint(utilsGetEnv("MINING_POOL_RPC", "Mining Pool RPC url")),
    pubkeyFromBase58("PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf"),
    expressApp,
  );
  coordinatorService(
    makeSolanaEndpoint(utilsGetEnv("COORDINATOR_RPC", "Coordinator RPC url")),
    pubkeyFromBase58("HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y"),
    expressApp,
  );
  coordinatorService(
    makeSolanaEndpoint(utilsGetEnv("COORDINATOR_RPC", "Coordinator RPC url")),
    pubkeyFromBase58("4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7"),
    expressApp,
  );
}

main();
