import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { coordinatorProcess } from "./coordinator/CoordinatorProcess";
import { miningPoolProcess } from "./mining-pool/MiningPoolProcess";

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
  coordinatorProcess(
    coordinatorCluster,
    coordinatorEndpoint,
    coordinatorProgramAddress,
  );
}

async function main2() {
  miningPoolProcess(
    miningPoolCluster,
    miningPoolEndpoint,
    miningPoolProgramAddress,
  );
}

main2();

export function withContext<T>(message: string, fn: () => T): T {
  try {
    return fn();
  } catch (error) {
    throw new Error(
      `${message}\n > ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}
