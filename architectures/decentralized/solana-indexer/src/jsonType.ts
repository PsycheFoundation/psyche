import { PublicKey } from "@solana/web3.js";
import {
  JsonArray,
  jsonExpectArray,
  jsonExpectBoolean,
  jsonExpectNull,
  jsonExpectNumber,
  jsonExpectObject,
  jsonExpectString,
  JsonObject,
  JsonValue,
} from "./json";
import { camelCaseToSnakeCase, Immutable, withContext } from "./utils";

export type JsonTypeContent<S> = S extends JsonType<infer T> ? T : never;
export type JsonTypeEncoder<Content> = {
  encode: (decoded: Immutable<Content>) => JsonValue;
};
export type JsonTypeDecoder<Content> = {
  decode: (encoded: Immutable<JsonValue>) => Content;
};
export type JsonType<Content> = {
  decode(encoded: Immutable<JsonValue>): Content;
  encode(decoded: Immutable<Content>): JsonValue;
};

// TODO - using classes would dampen allocation pressure

class JsonTypeConst<T extends number | string | boolean> {
  private expected: T;

  constructor(expected: T) {
    this.expected = expected;
  }
  decode(encoded: JsonValue): T {
    if (encoded !== this.expected) {
      throw new Error(
        `JSON: Expected const: ${this.expected} (found: ${encoded})`,
      );
    }
    return this.expected;
  }
  encode(_decoded: Immutable<T>): JsonValue {
    return this.expected;
  }
}
export function jsonTypeConst<N extends number | string | boolean>(
  expected: N,
): JsonType<N> {
  return new JsonTypeConst(expected);
}

const jsonTypeValueCached = {
  decode(encoded: JsonValue): JsonValue {
    return JSON.parse(JSON.stringify(encoded));
  },
  encode(decoded: Immutable<JsonValue>): JsonValue {
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
          () => shape[keyDecoded]!.decode(object[keyEncoded]!),
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

// TODO - better naming ?
export function jsonTypeWrap<Outer, Inner>(
  innerType: JsonType<Inner>,
  toOuter: (inner: Inner) => Outer,
  toInner: (outer: Immutable<Outer>) => Immutable<Inner>,
): JsonType<Outer> {
  return {
    decode(encoded: JsonValue): Outer {
      return toOuter(innerType.decode(encoded));
    },
    encode(decoded: Immutable<Outer>): JsonValue {
      return innerType.encode(toInner(decoded));
    },
  };
}
