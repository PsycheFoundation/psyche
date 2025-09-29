import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { coordinatorService } from "./coordinator/CoordinatorService";
import { miningPoolService } from "./mining-pool/MiningPoolService";

const miningPoolCluster = "mainnet";
const miningPoolEndpoint = new ToolboxEndpoint(
  "https://mainnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
  "confirmed",
);
const miningPoolProgramAddress = new PublicKey(
  "PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf",
);

const coordinatorCluster = "devnet";
const coordinatorEndpoint = new ToolboxEndpoint(
  "https://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722",
  "confirmed",
);
const coordinatorProgramAddress = new PublicKey(
  "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y",
);

async function main1() {
  coordinatorService(
    coordinatorCluster,
    coordinatorEndpoint,
    coordinatorProgramAddress,
  );
}

async function main2() {
  miningPoolService(
    miningPoolCluster,
    miningPoolEndpoint,
    miningPoolProgramAddress,
  );
}

main1();
main2();
