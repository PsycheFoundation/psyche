import {
  pubkeyFromBase58,
  rpcHttpFromUrl,
  rpcHttpWithRetryOnError,
} from "solana-kiss";
import { coordinatorService } from "./coordinator/CoordinatorService";
import { miningPoolService } from "./mining-pool/MiningPoolService";

function rpcHttpBuilder(url: string) {
  return rpcHttpWithRetryOnError(
    rpcHttpFromUrl(url, { commitment: "confirmed" }),
    async (error, context) => {
      if (context.retryCounter >= 5 || context.totalTimeMs >= 10000) {
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, 1000));
      console.error("RPC HTTP error occurred, retrying", error);
      return true;
    },
  );
}

const miningPoolCluster = "mainnet";
const miningPoolRpcHttp = rpcHttpBuilder(
  "https://mainnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
);
const miningPoolProgramAddress = pubkeyFromBase58(
  "PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf",
);

const coordinatorCluster = "devnet";
const coordinatorRpcHttp = rpcHttpBuilder(
  "https://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
);
const coordinatorProgramAddress = pubkeyFromBase58(
  "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y",
);

async function coordinatorMain() {
  coordinatorService(
    coordinatorCluster,
    coordinatorRpcHttp,
    coordinatorProgramAddress,
  );
}

async function miningPoolMain() {
  miningPoolService(
    miningPoolCluster,
    miningPoolRpcHttp,
    miningPoolProgramAddress,
  );
}

coordinatorMain();
miningPoolMain();
