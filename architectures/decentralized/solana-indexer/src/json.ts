import { PublicKey } from "@solana/web3.js";
import { camelCaseToSnakeCase, Immutable, withContext } from "./utils";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonArray
  | JsonObject;
export type JsonArray = Array<JsonValue>;
export interface JsonObject {
  [key: string]: JsonValue;
}

export function jsonPreview(value: JsonValue): string {
  if (jsonAsNull(value) !== undefined) {
    return "Null";
  }
  const boolean = jsonAsBoolean(value);
  if (boolean !== undefined) {
    return `Boolean: ${boolean}`;
  }
  const number = jsonAsNumber(value);
  if (number !== undefined) {
    return `Number: ${number}`;
  }
  const string = jsonAsString(value);
  if (string !== undefined) {
    return `String: "${string}"`;
  }
  const maxColumns = 40;
  const array = jsonAsArray(value);
  if (array !== undefined) {
    let previews = array.map(jsonPreview).join(", ");
    if (previews.length > maxColumns) {
      previews = previews.slice(0, maxColumns - 3) + "...";
    }
    return `Array(x${array.length}): [${previews}]`;
  }
  const object = jsonAsObject(value);
  if (object !== undefined) {
    const entries = Object.entries(object);
    let previews = entries
      .map(([key, val]) => `${key}: ${jsonPreview(val)}`)
      .join(", ");
    if (previews.length > maxColumns) {
      previews = previews.slice(0, maxColumns - 3) + "...";
    }
    return `Object(x${entries.length}): {${previews}}`;
  }
  throw new Error(`JSON: Unknown value: ${value?.toString()}`);
}

export function jsonAsNull(value: JsonValue): null | undefined {
  if (value === null) {
    return null;
  }
  return undefined;
}
export function jsonAsBoolean(value: JsonValue): boolean | undefined {
  if (typeof value === "boolean" || value instanceof Boolean) {
    return value as boolean;
  }
  return undefined;
}
export function jsonAsNumber(value: JsonValue): number | undefined {
  if (typeof value === "number" || value instanceof Number) {
    return value as number;
  }
  return undefined;
}
export function jsonAsString(value: JsonValue): string | undefined {
  if (typeof value === "string" || value instanceof String) {
    return value as string;
  }
  return undefined;
}
export function jsonAsArray(value: JsonValue): JsonArray | undefined {
  if (Array.isArray(value)) {
    return value as JsonArray;
  }
  return undefined;
}
export function jsonAsObject(value: JsonValue): JsonObject | undefined {
  if (typeof value === "object" && !Array.isArray(value) && value !== null) {
    return value as JsonObject;
  }
  return undefined;
}

export function jsonObjectHasKey(object: JsonObject, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(object, key);
}

export function jsonExpectValueShallowEquals(
  found: JsonValue,
  expected: JsonValue,
) {
  if (found !== expected) {
    const foundPreview = jsonPreview(found);
    const expectedPreview = jsonPreview(expected);
    throw new Error(
      `JSON: Expected: ${expectedPreview} (found: ${foundPreview})`,
    );
  }
}

export function jsonExpectNull(value: JsonValue): null {
  const result = jsonAsNull(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected null (found: ${jsonPreview(value)})`);
  }
  return result;
}
export function jsonExpectBoolean(value: JsonValue): boolean {
  const result = jsonAsBoolean(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected a boolean (found: ${jsonPreview(value)})`);
  }
  return result;
}
export function jsonExpectNumber(value: JsonValue): number {
  const result = jsonAsNumber(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected a number (found: ${jsonPreview(value)})`);
  }
  return result;
}
export function jsonExpectString(value: JsonValue): string {
  const result = jsonAsString(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected a string (found: ${jsonPreview(value)})`);
  }
  return result;
}
export function jsonExpectArray(value: JsonValue): JsonArray {
  const result = jsonAsArray(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected an array (found: ${jsonPreview(value)})`);
  }
  return result;
}
export function jsonExpectObject(value: JsonValue): JsonObject {
  const result = jsonAsObject(value);
  if (result === undefined) {
    throw new Error(`JSON: Expected an object (found: ${jsonPreview(value)})`);
  }
  return result;
}

export function jsonExpectValueFromArray(
  array: JsonArray,
  index: number,
): JsonValue {
  const item = array[index];
  if (item === undefined) {
    throw new Error(
      `JSON: Expected value in array at index: ${index} (array length: ${array.length})`,
    );
  }
  return item;
}
export function jsonExpectValueFromObject(
  object: JsonObject,
  key: string,
): JsonValue {
  const value = object[key];
  if (value === undefined) {
    throw new Error(
      `JSON: Expected object to contain key "${key}" (object keys: ${Object.keys(object).join(", ")})`,
    );
  }
  return value;
}

export type JsonTypeContent<S> = S extends JsonType<infer T> ? T : never;
export type JsonType<Content> = {
  decode(encoded: JsonValue): Content;
  encode(decoded: Immutable<Content>): JsonValue;
};

export function jsonTypeConst<Const extends number | string | boolean>(
  expected: Const,
): JsonType<Const> {
  return {
    decode(encoded: JsonValue): Const {
      if (encoded !== expected) {
        throw new Error(
          `JSON: Expected const: ${expected} (found: ${encoded})`,
        );
      }
      return expected;
    },
    encode(): JsonValue {
      return expected;
    },
  };
}

const jsonTypeValueCached = {
  decode(encoded: JsonValue): JsonValue {
    return JSON.parse(JSON.stringify(encoded));
  },
  encode(decoded: JsonValue): JsonValue {
    return JSON.parse(JSON.stringify(decoded));
  },
};
export function jsonTypeValue(): JsonType<JsonValue> {
  return jsonTypeValueCached;
}

const jsonTypeNullCached = {
  decode(encoded: JsonValue): null {
    return jsonExpectNull(encoded);
  },
  encode(decoded: Immutable<null>): JsonValue {
    return decoded;
  },
};
export function jsonTypeNull(): JsonType<null> {
  return jsonTypeNullCached;
}

const jsonTypeBooleanCached = {
  decode(encoded: JsonValue): boolean {
    return jsonExpectBoolean(encoded);
  },
  encode(decoded: Immutable<boolean>): JsonValue {
    return decoded;
  },
};
export function jsonTypeBoolean(): JsonType<boolean> {
  return jsonTypeBooleanCached;
}

const jsonTypeNumberCached = {
  decode(encoded: JsonValue): number {
    return jsonExpectNumber(encoded);
  },
  encode(decoded: Immutable<number>): JsonValue {
    return decoded;
  },
};
export function jsonTypeNumber(): JsonType<number> {
  return jsonTypeNumberCached;
}

const jsonTypeStringCached = {
  decode(encoded: JsonValue): string {
    return jsonExpectString(encoded);
  },
  encode(decoded: Immutable<string>): JsonValue {
    return decoded;
  },
};
export function jsonTypeString(): JsonType<string> {
  return jsonTypeStringCached;
}

const jsonTypeStringToPubkeyCached = {
  decode(encoded: JsonValue): PublicKey {
    return new PublicKey(jsonExpectString(encoded));
  },
  encode(decoded: Immutable<PublicKey>): JsonValue {
    return String(decoded);
  },
};
export function jsonTypeStringToPubkey(): JsonType<PublicKey> {
  return jsonTypeStringToPubkeyCached;
}

const jsonTypeStringToBigintCached = {
  decode(encoded: JsonValue): bigint {
    return BigInt(jsonExpectString(encoded));
  },
  encode(decoded: Immutable<bigint>): JsonValue {
    return String(decoded);
  },
};
export function jsonTypeStringToBigint(): JsonType<bigint> {
  return jsonTypeStringToBigintCached;
}

export function jsonTypeArray<Item>(
  itemType: JsonType<Item>,
): JsonType<Array<Item>> {
  return {
    decode(encoded: JsonValue): Array<Item> {
      return jsonExpectArray(encoded).map((item, index) =>
        withContext(`JSON: Decode Array[${index}] =>`, () =>
          itemType.decode(item),
        ),
      );
    },
    encode(decoded: Immutable<Array<Item>>): Array<JsonValue> {
      return decoded.map((item) => itemType.encode(item));
    },
  };
}

export function jsonTypeObject<Shape extends { [key: string]: JsonType<any> }>(
  shape: Shape,
  keyEncoder: (key: string) => string = camelCaseToSnakeCase,
): JsonType<{ [K in keyof Shape]: JsonTypeContent<Shape[K]> }> {
  return {
    decode(encoded: JsonValue): {
      [K in keyof Shape]: JsonTypeContent<Shape[K]>;
    } {
      const object = jsonExpectObject(encoded);
      const decoded = {} as { [K in keyof Shape]: JsonTypeContent<Shape[K]> };
      for (const keyDecoded in shape) {
        const keyEncoded = keyEncoder ? keyEncoder(keyDecoded) : keyDecoded;
        decoded[keyDecoded] = withContext(
          `JSON: Decode Object["${keyEncoded}"] =>`,
          () =>
            shape[keyDecoded]!.decode(
              jsonExpectValueFromObject(object, keyEncoded),
            ),
        );
      }
      return decoded;
    },
    encode(
      decoded: Immutable<{
        [K in keyof Shape]: JsonTypeContent<Shape[K]>;
      }>,
    ): JsonValue {
      const encoded = {} as JsonObject;
      for (const keyDecoded in shape) {
        const keyEncoded = keyEncoder ? keyEncoder(keyDecoded) : keyDecoded;
        encoded[keyEncoded] = shape[keyDecoded]!.encode(
          decoded[keyDecoded as keyof typeof decoded],
        );
      }
      return encoded;
    },
  };
}

export function jsonTypeObjectToRecord<Value>(
  valueType: JsonType<Value>,
): JsonType<Record<string, Value>> {
  return {
    decode(encoded: JsonValue): Record<string, Value> {
      const object = jsonExpectObject(encoded);
      const decoded: Record<string, Value> = {};
      for (const key in object) {
        decoded[key] = withContext(`JSON: Decode Object["${key}"] =>`, () =>
          valueType.decode(object[key]!),
        );
      }
      return decoded;
    },
    encode(decoded: Immutable<Record<string, Value>>): JsonValue {
      const encoded = {} as JsonObject;
      for (const [key, value] of Object.entries(decoded)) {
        encoded[key] = valueType.encode(value);
      }
      return encoded;
    },
  };
}

export function jsonTypeObjectToMap<Value>(
  valueType: JsonType<Value>,
): JsonType<Map<string, Value>> {
  return {
    decode(encoded: JsonValue): Map<string, Value> {
      const object = jsonExpectObject(encoded);
      const decoded = new Map<string, Value>();
      for (const key in object) {
        decoded.set(
          key,
          withContext(`JSON: Decode Object["${key}"] =>`, () =>
            valueType.decode(object[key]!),
          ),
        );
      }
      return decoded;
    },
    encode(decoded: Immutable<Map<string, Value>>): JsonValue {
      const encoded = {} as JsonObject;
      for (const [key, val] of decoded.entries()) {
        encoded[key] = valueType.encode(val);
      }
      return encoded;
    },
  };
}

export function jsonTypeArrayToMap<Key, Value>(
  keyType: JsonType<Key>,
  valueType: JsonType<Value>,
): JsonType<Map<Key, Value>> {
  return {
    decode(encoded: JsonValue): Map<Key, Value> {
      const array = jsonExpectArray(encoded);
      const decoded = new Map<Key, Value>();
      for (let i = 0; i < array.length; i++) {
        const item = array[i]!;
        const keyValue = jsonExpectArray(item);
        if (keyValue.length !== 2) {
          throw new Error(`JSON: Expected key-value array of length 2`);
        }
        decoded.set(
          withContext(`JSON: Decode Array[${i}]["key"] =>`, () =>
            keyType.decode(keyValue[0]!),
          ),
          withContext(`JSON: Decode Array[${i}]["value"] =>`, () =>
            valueType.decode(keyValue[1]!),
          ),
        );
      }
      return decoded;
    },
    encode(decoded: Immutable<Map<Key, Value>>): JsonValue {
      const encoded: JsonArray = [];
      for (const [key, val] of decoded.entries()) {
        encoded.push([keyType.encode(key), valueType.encode(val)]);
      }
      encoded.sort();
      return encoded;
    },
  };
}

export function jsonTypeNullable<Content>(
  contentType: JsonType<Content>,
): JsonType<Content | null> {
  return {
    decode(encoded: JsonValue): Content | null {
      if (encoded === null) {
        return null;
      }
      return contentType.decode(encoded);
    },
    encode(decoded: Immutable<Content | null>): JsonValue {
      if (decoded === null) {
        return null;
      }
      return contentType.encode(decoded);
    },
  };
}

export function jsonTypeNullableToOptional<Content>(
  contentType: JsonType<Content>,
): JsonType<Content | undefined> {
  return {
    decode(encoded: JsonValue): Content | undefined {
      if (encoded === null) {
        return undefined;
      }
      return contentType.decode(encoded);
    },
    encode(decoded: Immutable<Content | undefined>): JsonValue {
      if (decoded === undefined) {
        return null;
      }
      return contentType.encode(decoded);
    },
  };
}

export function jsonTypeMapped<Mapped, Unmapped>(
  unmappedType: JsonType<Unmapped>,
  processors: {
    map: (unmapped: Unmapped) => Mapped;
    unmap: (mapped: Immutable<Mapped>) => Immutable<Unmapped>;
  },
): JsonType<Mapped> {
  return {
    decode(encoded: JsonValue): Mapped {
      return processors.map(unmappedType.decode(encoded));
    },
    encode(decoded: Immutable<Mapped>): JsonValue {
      return unmappedType.encode(processors.unmap(decoded));
    },
  };
}

export function jsonTypeObjectToVariant<Variant>(
  variantKey: string,
  variantType: JsonType<Variant>,
): JsonType<Variant> {
  return jsonTypeMapped(jsonTypeObject({ [variantKey]: variantType }), {
    map: (unmapped) => unmapped[variantKey]!,
    unmap: (mapped) => ({ [variantKey]: mapped }),
  });
}

export function jsonTypeWithDecodeFallbacks<Content>(
  currentType: JsonType<Content>,
  decodeFallbacks: Array<(value: JsonValue) => Content>,
): JsonType<Content> {
  return {
    decode(encoded: JsonValue): Content {
      const errors = [];
      try {
        return currentType.decode(encoded);
      } catch (error) {
        errors.push(error);
      }
      for (const decodeFallback of decodeFallbacks) {
        try {
          return decodeFallback(encoded);
        } catch (error) {
          errors.push(error);
        }
      }
      const separator = "\n---\n >> JSON: Decode error: ";
      throw new Error(
        `JSON: Decode with fallbacks failed: ${separator}${errors.join(separator)})`,
      );
    },
    encode: currentType.encode,
  };
}
