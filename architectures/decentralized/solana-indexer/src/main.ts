import { rpcHttpFromUrl } from "solana-kiss-rpc";
import { coordinatorService } from "./coordinator/CoordinatorService";
import { miningPoolService } from "./mining-pool/MiningPoolService";

const miningPoolCluster = "mainnet";
const miningPoolRpcHttp = rpcHttpFromUrl(
  "https://mainnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
  { commitment: "confirmed" },
);
const miningPoolProgramAddress = "PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf";

const coordinatorCluster = "devnet";
const coordinatorRpcHttp = rpcHttpFromUrl(
  "https://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
  { commitment: "confirmed" },
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
