import { rpcHttpFromUrl, rpcHttpWithRetryOnError } from "solana-kiss-rpc";
import { coordinatorService } from "./coordinator/CoordinatorService";
import { miningPoolService } from "./mining-pool/MiningPoolService";

function rpcHttpBuilder(url: string) {
  return rpcHttpWithRetryOnError(
    rpcHttpFromUrl(url, { commitment: "confirmed" }),
    (retryCount, error) => {
      if (retryCount > 5) {
        console.error("RPC HTTP max retries reached, aborting", error);
      }
      return 1000;
    },
  );
}

const miningPoolCluster = "mainnet";
const miningPoolRpcHttp = rpcHttpBuilder(
  "https://mainnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
);
const miningPoolProgramAddress = "PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf";

const coordinatorCluster = "devnet";
const coordinatorRpcHttp = rpcHttpBuilder(
  "https://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
);
const coordinatorProgramAddress =
  "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y";

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
