import {
  JsonType,
  jsonTypeArray,
  jsonTypeArrayToTuple,
  jsonTypeMapped,
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeObjectWithKeyEncoder,
  jsonTypeStringToBigint,
} from "./json";

export function withContext<T>(message: string, fn: () => T): T {
  try {
    return fn();
  } catch (error) {
    throw new Error(
      `${message}\n > ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

export function camelCaseToSnakeCase(str: string): string {
  return str
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2") // insert underscore before capital letters
    .toLowerCase();
}

export type Immutable<T> = T extends
  | string
  | number
  | boolean
  | bigint
  | symbol
  | null
  | undefined
  | Date
  | RegExp
  | Error
  ? T
  : T extends (...args: any[]) => any
    ? T
    : T extends ReadonlyMap<infer K, infer V>
      ? ReadonlyMap<Immutable<K>, Immutable<V>>
      : T extends Map<infer K, infer V>
        ? ReadonlyMap<Immutable<K>, Immutable<V>>
        : T extends ReadonlySet<infer U>
          ? ReadonlySet<Immutable<U>>
          : T extends Set<infer U>
            ? ReadonlySet<Immutable<U>>
            : T extends WeakMap<infer K, infer V>
              ? WeakMap<Immutable<K>, Immutable<V>>
              : T extends WeakSet<infer U>
                ? WeakSet<Immutable<U>>
                : T extends Promise<infer U>
                  ? Promise<Immutable<U>>
                  : T extends readonly []
                    ? T
                    : T extends readonly [infer _H, ...infer _R]
                      ? { readonly [I in keyof T]: Immutable<T[I]> }
                      : T extends ReadonlyArray<infer U>
                        ? ReadonlyArray<Immutable<U>>
                        : T extends Array<infer U>
                          ? ReadonlyArray<Immutable<U>>
                          : T extends object
                            ? { readonly [P in keyof T]: Immutable<T[P]> }
                            : T;

export function jsonTypeObjectSnakeCase<T>(fields: {
  [key in keyof T]: JsonType<T[key]>;
}) {
  return jsonTypeObjectWithKeyEncoder(fields, camelCaseToSnakeCase);
}

export function jsonTypeRustFixedString() {
  return jsonTypeMapped(
    jsonTypeArrayToTuple([jsonTypeArray(jsonTypeNumber())]),
    {
      map: (unmapped) => {
        const bytes = unmapped[0];
        const nulIndex = bytes.indexOf(0);
        const trimmed = nulIndex >= 0 ? bytes.slice(0, nulIndex) : bytes;
        return {
          value: new TextDecoder().decode(new Uint8Array(trimmed)),
          length: bytes.length,
        };
      },
      unmap: (mapped) => {
        const bytes = new TextEncoder().encode(mapped.value);
        const padded = new Uint8Array(mapped.length);
        padded.set(bytes);
        return [Array.from(padded)] as [number[]];
      },
    },
  );
}

export function jsonTypeRustFixedArray<T>(itemType: JsonType<T>) {
  return jsonTypeMapped(
    jsonTypeObject({
      data: jsonTypeArray(itemType),
      len: jsonTypeStringToBigint(),
    }),
    {
      map: (unmapped) => unmapped.data.slice(0, Number(unmapped.len)),
      unmap: (mapped) => ({ data: mapped, len: BigInt(mapped.length) }),
    },
  );
}
