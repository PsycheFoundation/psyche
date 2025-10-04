import { Pubkey, Signature } from "solana-kiss-data";
import { RpcHttp, rpcHttpFindAccountPastSignatures } from "solana-kiss-rpc";
import { IndexingCheckpoint } from "./IndexingCheckpoint";

export async function indexingSignaturesLoop(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  onSignature: (signature: Signature, ordering: bigint) => Promise<void>,
  onCheckpoint: (indexedCheckpoint: IndexingCheckpoint) => Promise<void>,
): Promise<never> {
  const indexedChunks = startingCheckpoint.indexedChunks.map((c) => ({ ...c }));
  while (true) {
    await onCheckpoint({
      indexedChunks: indexedChunks.map((c) => ({ ...c })),
    });
    const prevChunkIndex =
      Math.floor(Math.random() * (indexedChunks.length + 1)) - 1;
    const nextChunkIndex = prevChunkIndex + 1;
    const prevChunkInfo = indexedChunks[prevChunkIndex];
    const nextChunkInfo = indexedChunks[nextChunkIndex];
    const signatures = await rpcHttpFindAccountPastSignatures(
      rpcHttp,
      programAddress,
      1000,
      {
        startBeforeSignature: prevChunkInfo?.rewindedUntil,
        rewindUntilSignature: nextChunkInfo?.startedFrom,
      },
    );
    if (signatures.length === 0) {
      continue;
    }
    const orderingHigh = prevChunkInfo
      ? prevChunkInfo.orderingLow
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
    if (prevChunkInfo !== undefined) {
      prevChunkInfo.rewindedUntil = rewindedUntil;
      prevChunkInfo.orderingLow = orderingLow;
      prevChunkInfo.processedCounter += processedCounter;
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
