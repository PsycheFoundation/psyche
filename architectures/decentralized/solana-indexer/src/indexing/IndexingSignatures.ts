import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { IndexingCheckpoint } from "./IndexingCheckpoint";

export async function indexingSignaturesLoop(
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  onSignature: (
    signature: TransactionSignature,
    ordering: bigint,
  ) => Promise<void>,
  onCheckpoint: (indexedCheckpoint: IndexingCheckpoint) => Promise<void>,
): Promise<never> {
  const indexedChunks = startingCheckpoint.indexedChunks.map((c) => ({ ...c }));
  while (true) {
    await onCheckpoint({
      indexedChunks: indexedChunks.map((c) => ({ ...c })),
    });
    const currChunkIndex =
      Math.floor(Math.random() * (indexedChunks.length + 1)) - 1;
    const nextChunkIndex = currChunkIndex + 1;
    const currChunkInfo = indexedChunks[currChunkIndex];
    const nextChunkInfo = indexedChunks[nextChunkIndex];
    const signatures = await endpoint.searchSignatures(
      programAddress,
      1000,
      currChunkInfo?.rewindedUntil,
      nextChunkInfo?.startedFrom,
    );
    if (signatures.length === 0) {
      continue;
    }
    const orderingHigh = currChunkInfo
      ? currChunkInfo.orderingLow
      : BigInt(new Date().getTime()) * 1000000n;
    let orderingLow = orderingHigh - BigInt(signatures.length);
    let processedCounter = signatures.length;
    const startedFrom = signatures[0]!;
    let rewindedUntil = signatures[signatures.length - 1]!;
    if (rewindedUntil === nextChunkInfo?.startedFrom) {
      rewindedUntil = nextChunkInfo.rewindedUntil;
      orderingLow = nextChunkInfo.orderingLow;
      processedCounter += nextChunkInfo.processedCounter - 1;
      indexedChunks.splice(nextChunkIndex, 1);
      signatures.pop();
    }
    if (currChunkInfo !== undefined) {
      currChunkInfo.rewindedUntil = rewindedUntil;
      currChunkInfo.orderingLow = orderingLow;
      currChunkInfo.processedCounter += processedCounter;
    } else {
      indexedChunks.unshift({
        orderingHigh: orderingHigh,
        orderingLow: orderingLow,
        startedFrom: startedFrom,
        rewindedUntil: rewindedUntil,
        processedCounter: processedCounter,
      });
    }
    const promises = new Array<Promise<void>>();
    for (let i = 0; i < signatures.length; i++) {
      const signature = signatures[i]!;
      const ordering = orderingHigh - BigInt(i);
      promises.push(onSignature(signature, ordering));
    }
    await Promise.all(promises);
  }
}
