import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { syncMiningPool } from "./miningPool";

const miningPoolEndpoint = new ToolboxEndpoint("mainnet", "confirmed");
const miningPoolProgramAddress = new PublicKey(
  "PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf",
);

async function main() {
  const idlProgram = await new ToolboxIdlService().getOrResolveProgram(
    miningPoolEndpoint,
    miningPoolProgramAddress,
  );
  if (!idlProgram) {
    throw new Error("Failed to fetch IDL for mining pool program");
  }
  await syncMiningPool(
    miningPoolEndpoint,
    idlProgram,
    miningPoolProgramAddress,
  );
}

main();
