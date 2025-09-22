import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlProgram } from "solana_toolbox_web3";
import { exploreSignaturesLoop } from "./exploreSignatures";

export async function syncMiningPool(
  endpoint: ToolboxEndpoint,
  idlProgram: ToolboxIdlProgram,
  programAddress: PublicKey,
) {
  const orderedExploredChunks: Array<{
    startedFrom: TransactionSignature;
    rewindedUntil: TransactionSignature;
  }> = [];
  const depositPerUserPerPool = new Map<string, Map<string, bigint>>();
  await exploreSignaturesLoop(
    endpoint,
    programAddress,
    1,
    5000,
    orderedExploredChunks,
    async (signatures) => {
      for (const signature of signatures) {
        try {
          const execution = await endpoint.getExecution(signature);
          console.log("-- mp:", signature, execution.slot);
          for (const instruction of execution.instructions) {
            if (!instruction.programId.equals(programAddress)) {
              continue;
            }
            const idlInstruction = idlProgram.guessInstruction(
              instruction.data,
            );
            if (!idlInstruction) {
              continue;
            }
            const { instructionAddresses, instructionPayload } =
              idlInstruction.decode(instruction);
            if (idlInstruction.name === "lender_deposit") {
              const pool = instructionAddresses.get("pool");
              const user = instructionAddresses.get("user");
              const amount = BigInt(
                instructionPayload["params"]["collateral_amount"],
              );
              console.log(
                "lender_deposit",
                amount,
                user?.toBase58(),
                pool?.toBase58(),
              );
              if (amount && user && pool) {
                let depositPerUser = depositPerUserPerPool.get(pool.toBase58());
                if (!depositPerUser) {
                  depositPerUser = new Map<string, bigint>();
                  depositPerUserPerPool.set(pool.toBase58(), depositPerUser);
                }
                const deposit = depositPerUser.get(user.toBase58()) ?? 0n;
                depositPerUser.set(user.toBase58(), deposit + BigInt(amount));
              }
            }
          }
        } catch (e) {
          console.error("------", signature, "ERR", e);
        }
      }
      console.log("depositPerUserPerPool", depositPerUserPerPool);
    },
  );
}
