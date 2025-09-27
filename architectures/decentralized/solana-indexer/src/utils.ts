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

export function identity<T>(value: T): T {
  return value;
}

// Deep readonly (handles Map/Set/Array/Tuple/Promise + plain objects)
export type Immutable<T> =
  // leave primitives & common builtins as-is
  T extends
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
    : // functions stay callable
      T extends (...args: any[]) => any
      ? T
      : T extends ReadonlyMap<infer K, infer V>
        ? ReadonlyMap<Immutable<K>, Immutable<V>>
        : T extends Map<infer K, infer V>
          ? ReadonlyMap<Immutable<K>, Immutable<V>>
          : // Set / ReadonlySet -> ReadonlySet (deep on T)
            T extends ReadonlySet<infer U>
            ? ReadonlySet<Immutable<U>>
            : T extends Set<infer U>
              ? ReadonlySet<Immutable<U>>
              : // WeakMap/WeakSet have no readonly counterparts; keep types but deep the params
                T extends WeakMap<infer K, infer V>
                ? WeakMap<Immutable<K>, Immutable<V>>
                : T extends WeakSet<infer U>
                  ? WeakSet<Immutable<U>>
                  : // Promise deepens its value
                    T extends Promise<infer U>
                    ? Promise<Immutable<U>>
                    : // Preserve tuples exactly (don’t widen)
                      T extends readonly []
                      ? T
                      : T extends readonly [infer _H, ...infer _R]
                        ? { readonly [I in keyof T]: Immutable<T[I]> }
                        : // Arrays
                          T extends ReadonlyArray<infer U>
                          ? ReadonlyArray<Immutable<U>>
                          : T extends Array<infer U>
                            ? ReadonlyArray<Immutable<U>>
                            : // Plain objects
                              T extends object
                              ? { readonly [P in keyof T]: Immutable<T[P]> }
                              : T;
