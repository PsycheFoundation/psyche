import crypto from "node:crypto";

function sha256(data: Uint8Array): Uint8Array {
  const hash = crypto.createHash("sha256");
  hash.update(Buffer.from(data));
  return new Uint8Array(hash.digest());
}

// concat and hash multiple byte arrays / utf8 strings
function sha256v(parts: (Uint8Array | number[] | string)[]): Uint8Array {
  const totalLength = parts.reduce((acc, part) => {
    if (typeof part === "string") {
      return acc + new TextEncoder().encode(part).length;
    }
    return acc + part.length;
  }, 0);

  const combined = new Uint8Array(totalLength);
  let offset = 0;

  for (const part of parts) {
    let bytes: Uint8Array;
    if (typeof part === "string") {
      bytes = new TextEncoder().encode(part);
    } else {
      bytes = new Uint8Array(part);
    }

    combined.set(bytes, offset);
    offset += bytes.length;
  }

  return sha256(combined);
}

function computeShuffledIndex(
  index: bigint,
  indexCount: bigint,
  seed: Uint8Array,
): bigint {
  if (index >= indexCount) {
    throw new Error("Index out of bounds");
  }

  const SHUFFLE_ROUND_COUNT = 90;
  let currentIndex = index;

  for (
    let currentRound = 0;
    currentRound < SHUFFLE_ROUND_COUNT;
    currentRound++
  ) {
    const roundBytes = new Uint8Array([currentRound]);
    const hashResult = sha256v([seed, roundBytes]);

    // convert first 8 bytes to uint64 (bigint since we don't have 64 in js)
    const pivot =
      BigInt(
        new DataView(hashResult.buffer.slice(0, 8)).getBigUint64(0, true),
      ) % indexCount;

    const flip = (pivot + indexCount - currentIndex) % indexCount;
    const position = currentIndex >= flip ? currentIndex : flip;

    const positionDiv256Buffer = new ArrayBuffer(4);
    const positionDiv256View = new DataView(positionDiv256Buffer);
    positionDiv256View.setUint32(0, Number(position / 256n), true); // true for little-endian
    const positionDiv256 = new Uint8Array(positionDiv256Buffer);

    const source = sha256v([seed, roundBytes, positionDiv256.slice(0, 4)]);

    const byte = source[Number((position % 256n) / 8n)];
    const bit = (byte >> Number(position % 8n)) % 2;

    currentIndex = bit === 1 ? flip : currentIndex;
  }

  return currentIndex;
}

/**
 * This replicates some of the logic from `committee_selection.rs` - TODO just export the original Rust code thru WASM
 * @param nodeIndex the client's index in the clients list
 * @param roundSeed the seed for this round
 * @param numTotalNodes the total number of clients
 * @param numWitnessNodes the number of witness nodes
 * @returns whether or not the client is a witness
 */
export function isClientWitness(
  nodeIndex: number,
  roundSeed: bigint,
  numTotalNodes: number,
  numWitnessNodes: number,
): boolean {
  const WITNESS_SALT = "witness"; // TODO figure out how we can keep this in sync with the Rust code, in case it changes.

  const clientIndexBigInt = BigInt(nodeIndex);
  const totalNodesBigInt = BigInt(numTotalNodes);

  const realRoundSeed = sha256(bigIntToLeU64Bytes(roundSeed));
  const witnessShuffleSeed = sha256v([realRoundSeed, WITNESS_SALT]);

  const shuffledPosition = computeShuffledIndex(
    clientIndexBigInt,
    totalNodesBigInt,
    witnessShuffleSeed,
  );

  if (numWitnessNodes === 0) {
    return true; // all nodes are witnesses
  } else {
    return shuffledPosition < BigInt(numWitnessNodes);
  }
}

export function bigIntToLeU64Bytes(value: bigint): Uint8Array {
  const buffer = new ArrayBuffer(8);
  const view = new DataView(buffer);
  view.setBigUint64(0, value, true);
  return new Uint8Array(buffer);
}
