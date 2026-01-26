import { PublicKey } from "@solana/web3.js";
import { Format } from "./type.js";
// NOTE: You CANNOT use information outside of this file for serialization / deserialization
// otherwise you could break old versions

interface PublicKeyJSON {
  ___type: "pubkey";
  value: string;
}

interface MapJSON {
  ___type: "map";
  value: Array<[string, any]>;
}

interface SetJSON {
  ___type: "set";
  value: any[];
}

function isMapJson(obj: any): obj is MapJSON {
  return (
    obj &&
    typeof obj === "object" &&
    obj.___type === "map" &&
    Array.isArray(obj.value)
  );
}

function isSetJson(obj: any): obj is SetJSON {
  return (
    obj &&
    typeof obj === "object" &&
    obj.___type === "set" &&
    Array.isArray(obj.value)
  );
}

function isPublicKeyJson(obj: any): obj is PublicKeyJSON {
  return (
    obj &&
    typeof obj === "object" &&
    obj.___type === "pubkey" &&
    typeof obj.value === "string"
  );
}

function isBN(obj: any) {
  return (
    obj &&
    typeof obj === "object" &&
    (("negative" in obj && "words" in obj && "length" in obj && "red" in obj) ||
      "_bn" in obj)
  );
}
function isPublicKey(obj: any) {
  return (
    obj &&
    typeof obj === "object" &&
    "constructor" in obj &&
    "findProgramAddressSync" in obj.constructor
  );
}

/// not the greatest thing to do but it's gonna make wayyyy smaller data than objects.
// if the data has ࿏ things will explode very badly.
const SIGIL = "࿏";

function psycheJsonReplacer(this: any, key: string): any {
  const value = this[key];
  if (isPublicKey(value)) {
    return {
      ___type: "pubkey",
      value: value.toString(),
    };
  }
  if (value instanceof Map) {
    return {
      ___type: "map",
      value: [...value.entries()],
    };
  }
  if (value instanceof Set) {
    return {
      ___type: "set",
      value: [...value.values()],
    };
  }

  if (typeof value === "bigint" || isBN(value)) {
    return `${SIGIL}BIG${value.toString()}`;
  }
  if (value instanceof Date) {
    return `${SIGIL}DAT${value.getTime()}`;
  }

  // drop sig figs if not needed
  const maxSigFigs = 7;
  if (
    typeof value === "number" &&
    isFinite(value) &&
    value !== 0 &&
    !Number.isInteger(value)
  ) {
    // if the number is "reasonable" sized, just use toPrecision
    const abs = Math.abs(value);
    if (abs >= 1e-4 && abs < 1e6) {
      const truncated = Number(value.toPrecision(maxSigFigs));
      // Only return truncated if it's actually different (avoiding unnecessary precision loss)
      return truncated === value ? value : truncated;
    } else {
      // For very large/small numbers, always truncate
      return Number(value.toPrecision(maxSigFigs));
    }
  }

  return value;
}

function psycheJsonReviver(_key: string, value: any): any {
  if (isPublicKeyJson(value)) {
    return new PublicKey(value.value);
  }
  if (isMapJson(value)) {
    return new Map(value.value);
  }
  if (isSetJson(value)) {
    return new Set(value.value);
  }
  const isCustom = typeof value === "string" && value[0] === SIGIL;
  if (!isCustom) {
    return value;
  }
  const id = value.slice(1, 4);
  const data = value.slice(4);
  if (id === "BIG") {
    return BigInt(data);
  }
  if (id === "DAT") {
    return new Date(+data);
  }
  throw new Error(`Unknown sigil type ${id} in value ${value}`);
}

export const format: Format = {
  replacer: psycheJsonReplacer,
  reviver: psycheJsonReviver,
};
