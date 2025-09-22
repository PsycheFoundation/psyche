import { PublicKey, TransactionSignature } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import {
  IndexingCheckpoint,
  IndexingCheckpointChunk,
} from "./IndexingCheckpoint";

export async function indexingSignaturesLoop(
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  onSignature: (
    signature: TransactionSignature,
    ordering: bigint,
  ) => Promise<void>,
  onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>,
): Promise<never> {
  const indexedOrderedChunks = startingCheckpoint.indexedOrderedChunks.slice();
  while (true) {
    await indexingSignaturesUntilNow(
      endpoint,
      programAddress,
      indexedOrderedChunks,
      onSignature,
      onCheckpoint,
    );
    await new Promise((resolve) => setTimeout(resolve, 3333));
  }
}

async function indexingSignaturesUntilNow(
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  indexedOrderedChunks: Array<IndexingCheckpointChunk>,
  onSignature: (
    signature: TransactionSignature,
    ordering: bigint,
  ) => Promise<void>,
  onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>,
): Promise<void> {
  let currChunkIndex = -1;
  while (true) {
    const nextChunkIndex = currChunkIndex + 1;
    const currChunkInfo = indexedOrderedChunks[currChunkIndex];
    const nextChunkInfo = indexedOrderedChunks[nextChunkIndex];
    console.log("indexingSignaturesUntilNow", indexedOrderedChunks);
    const signatures = await endpoint.searchSignatures(
      programAddress,
      100,
      currChunkInfo?.rewindedUntil,
      nextChunkInfo?.startedFrom,
    );
    if (signatures.length === 0) {
      return;
    }
    let orderingHigh: bigint;
    if (currChunkInfo) {
      orderingHigh = currChunkInfo.orderingLow;
    } else if (nextChunkInfo) {
      orderingHigh = nextChunkInfo.orderingHigh + orderingMargin;
    } else {
      orderingHigh = orderingMargin;
    }
    let orderingLow = orderingHigh - BigInt(signatures.length);
    let processedCounter = signatures.length;
    const startedFrom = signatures[0]!;
    let rewindedUntil = signatures[signatures.length - 1]!;
    if (rewindedUntil === nextChunkInfo?.startedFrom) {
      rewindedUntil = nextChunkInfo.rewindedUntil;
      orderingHigh = nextChunkInfo.orderingHigh - BigInt(processedCounter - 1);
      orderingLow = nextChunkInfo.orderingLow;
      processedCounter += nextChunkInfo.processedCounter - 1;
      indexedOrderedChunks.splice(nextChunkIndex, 1);
      signatures.pop();
    }
    if (currChunkInfo !== undefined) {
      currChunkInfo.orderingLow = orderingLow;
      currChunkInfo.rewindedUntil = rewindedUntil;
      currChunkInfo.processedCounter += processedCounter;
    } else {
      indexedOrderedChunks.unshift({
        orderingHigh: orderingHigh,
        orderingLow: orderingLow,
        startedFrom: startedFrom,
        rewindedUntil: rewindedUntil,
        processedCounter: processedCounter,
      });
      currChunkIndex++;
    }
    const promises = new Array<Promise<void>>();
    console.log("signatures", signatures.length, orderingHigh);
    for (let i = 0; i < signatures.length; i++) {
      const signature = signatures[i]!;
      const ordering = orderingHigh - BigInt(i);
      promises.push(onSignature(signature, ordering));
    }
    await Promise.all(promises);
    await onCheckpoint(new IndexingCheckpoint(indexedOrderedChunks));
  }
}

const orderingMargin = 10000000000000000n;
