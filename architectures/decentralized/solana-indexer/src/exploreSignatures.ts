import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { delay } from "./utils";

export async function exploreSignaturesLoop(
  endpoint: ToolboxEndpoint,
  programId: PublicKey,
  signaturesPerRequest: number,
  betweenCycleDelayMs: number,
  orderedExploredChunks: Array<{
    startedFrom: TransactionSignature;
    rewindedUntil: TransactionSignature;
  }>,
  onUnorderedExploredSignatures: (
    signatures: Array<TransactionSignature>,
  ) => Promise<void>,
): Promise<never> {
  while (true) {
    await exploreSignaturesNow(
      endpoint,
      programId,
      signaturesPerRequest,
      orderedExploredChunks,
      onUnorderedExploredSignatures,
    );
    await delay(betweenCycleDelayMs);
  }
}

export async function exploreSignaturesNow(
  endpoint: ToolboxEndpoint,
  programId: PublicKey,
  signaturesPerRequest: number,
  orderedExploredChunks: Array<{
    startedFrom: TransactionSignature;
    rewindedUntil: TransactionSignature;
  }>,
  onUnorderedExploredSignatures: (
    signatures: Array<TransactionSignature>,
  ) => Promise<void>,
): Promise<void> {
  let currChunkIndex = -1;
  while (true) {
    const nextChunkIndex = currChunkIndex + 1;
    const currChunkInfo = orderedExploredChunks[currChunkIndex];
    const nextChunkInfo = orderedExploredChunks[nextChunkIndex];
    const signatures = await endpoint.searchSignatures(
      programId,
      signaturesPerRequest,
      currChunkInfo?.rewindedUntil,
      nextChunkInfo?.startedFrom,
    );
    if (signatures.length === 0) {
      return;
    }
    let lastSignature = signatures[signatures.length - 1]!;
    if (lastSignature === nextChunkInfo?.startedFrom) {
      lastSignature = nextChunkInfo.rewindedUntil;
      orderedExploredChunks.splice(nextChunkIndex, 1);
      signatures.pop();
    }
    if (currChunkInfo !== undefined) {
      currChunkInfo.rewindedUntil = lastSignature;
    } else {
      orderedExploredChunks.unshift({
        startedFrom: signatures[0]!,
        rewindedUntil: lastSignature,
      });
      currChunkIndex++;
    }
    await onUnorderedExploredSignatures(signatures);
  }
}
