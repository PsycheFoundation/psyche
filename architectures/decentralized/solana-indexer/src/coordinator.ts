import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { exploreSignaturesLoop } from "./exploreSignatures";

const coordinator1 = new PublicKey(
  "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y",
);

const endpoint = new ToolboxEndpoint("devnet", "confirmed");
const idlService = new ToolboxIdlService();

async function onUnorderedSignatures(signatures: Array<TransactionSignature>) {
  const coordinatorAccounts = new Set<string>();
  for (const signature of signatures) {
    await onUnorderedSignature(signature, coordinatorAccounts);
  }
  for (const coordinatorAccount of coordinatorAccounts) {
    const coordinatorAccountPubkey = new PublicKey(coordinatorAccount);
    const accountInfo = await idlService.getAndInferAndDecodeAccount(
      endpoint,
      coordinatorAccountPubkey,
    );
    const runIdRaw = accountInfo.state.state.coordinator.run_id[0];
    const runIdBytes = Uint8Array.from(runIdRaw);
    const runIdString = Buffer.from(runIdBytes).toString("utf-8");
    console.log("runIdString", runIdString);
  }
}

async function onUnorderedSignature(
  signature: TransactionSignature,
  coordinatorAccounts: Set<string>,
) {
  try {
    const execution = await endpoint.getExecution(signature);
    console.log("------", signature, "OK", execution.slot);
    for (const instruction of execution.instructions) {
      if (!instruction.programId.equals(coordinator1)) {
        continue;
      }
      const instructionInfo = await idlService.inferAndDecodeInstruction(
        endpoint,
        instruction,
      );

      const coordinatorAccount = instructionInfo.instructionAddresses.get(
        "coordinator_account",
      );
      if (coordinatorAccount) {
        coordinatorAccounts.add(coordinatorAccount.toBase58());
      }

      /*
      if (instructionInfo.instruction.name === "witness") {
        console.log("witness", instructionInfo.instructionPayload.metadata);
      } else {
        console.log(
          "instructionInfo",
          instructionInfo.instruction.name,
          Array.from(instructionInfo.instructionAddresses.entries()).map(
            ([k, v]) => `${k}: ${v.toBase58()}`,
          ),
          instructionInfo.instructionPayload,
        );
      }
        */
    }
  } catch (error) {
    console.log("------", signature, "RPC ERROR", error);
  }
}

async function dudu() {
  // TODO - persist this between starts
  const orderedExploredChunks: Array<{
    startedFrom: TransactionSignature;
    rewindedUntil: TransactionSignature;
  }> = [];
  console.log("Hello, Solana Indexer!");
  await exploreSignaturesLoop(
    endpoint,
    coordinator1,
    5,
    5000,
    orderedExploredChunks,
    onUnorderedSignatures,
  );
  console.log("After sync call");
}
