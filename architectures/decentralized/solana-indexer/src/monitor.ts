import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxEndpointExecution } from "solana_toolbox_web3";
import { delay } from "./utils";

export async function incrementallyExploreTransactions(
  endpoint: ToolboxEndpoint,
  programId: PublicKey,
  chunkSize: number,
  exploredOrderedChunkRanges: Array<{
    startedFrom: TransactionSignature;
    rewindedUntil: TransactionSignature;
  }>,
  onUnorderedExploredTransaction: (
    signature: TransactionSignature,
    execution: ToolboxEndpointExecution,
  ) => Promise<void>,
): Promise<never> {
  let chunkIndex = 0;

  while (true) {
    const lastChunk = exploredOrderedChunkRanges[chunkIndex - 1];
    const currChunk = exploredOrderedChunkRanges[chunkIndex];

    let startBefore = lastChunk?.rewindedUntil;
    let rewindUntil = currChunk?.startedFrom;

    console.log("exploredOrderedChunkRanges", exploredOrderedChunkRanges);
    console.log("chunkIndex", chunkIndex);
    console.log("startBefore", startBefore);
    console.log("rewindUntil", rewindUntil);

    const signatures = await endpoint.searchSignatures(
      programId,
      chunkSize,
      startBefore,
      rewindUntil,
    );
    console.log("signatures.length", signatures.length);

    if (signatures.length > 0) {
      for (const signature of signatures) {
        await onUnorderedExploredTransaction(
          signature,
          await endpoint.getExecution(signature),
        );
      }

      const firstSignature = signatures[0]!;
      const lastSignature = signatures[signatures.length - 1]!;

      if (chunkIndex === 0) {
        exploredOrderedChunkRanges.unshift({
          startedFrom: firstSignature,
          rewindedUntil: lastSignature,
        });
      }

      if (lastSignature === rewindUntil) {
        if (chunkIndex === 0) {
          exploredOrderedChunkRanges[chuk];
        }
      } else {
      }
    }

    /*
    for (const signature of signatures) {
      console.log("signature", signature);
      const execution = await endpoint.getExecution(signature);
      console.log("execution", JSON.stringify(execution, null, 2));
      if (execution) {
        onTransaction(signature, execution);
      }
    }
      */
    await delay(1000);
    console.log("doing something...");
  }
}
