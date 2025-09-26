import { PublicKey } from "@solana/web3.js";
import {
  jsonExpectArray,
  jsonExpectBoolean,
  jsonExpectNull,
  jsonExpectNumber,
  jsonExpectObject,
  jsonExpectString,
  JsonValue,
} from "./json";
import { Immutable, withContext } from "./utils";

export type JsonTypeDecoder<Decoded> = {
  decode(encoded: Immutable<JsonValue>): Decoded;
};
export type JsonTypeEncoder<Encoded extends JsonValue, Decoded> = {
  encode(decoded: Immutable<Decoded>): Encoded;
};
export type JsonType<
  Encoded extends JsonValue,
  Decoded,
> = JsonTypeDecoder<Decoded> & JsonTypeEncoder<Encoded, Decoded>;

export type JsonTypeEncoded<S> =
  S extends JsonTypeEncoder<infer T, any> ? T : never;
export type JsonTypeDecoded<S> = S extends JsonTypeDecoder<infer T> ? T : never;

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
  encode(_decoded: Immutable<T>): T {
    return this.expected;
  }
}
export function jsonTypeConst<N extends number | string | boolean>(
  expected: N,
): JsonType<N, N> {
  return new JsonTypeConst(expected);
}

const jsonTypeValueCached = {
  decode(encoded: JsonValue): JsonValue {
    return encoded;
  },
  encode(decoded: Immutable<JsonValue>): JsonValue {
    return JSON.parse(JSON.stringify(decoded));
  },
};
export function jsonTypeValue(): JsonType<JsonValue, JsonValue> {
  return jsonTypeValueCached;
}

const jsonTypeNullCached = {
  decode(encoded: JsonValue): null {
    return jsonExpectNull(encoded);
  },
  encode(decoded: Immutable<null>): null {
    return decoded;
  },
};
export function jsonTypeNull(): JsonType<null, null> {
  return jsonTypeNullCached;
}

const jsonTypeBooleanCached = {
  decode(encoded: JsonValue): boolean {
    return jsonExpectBoolean(encoded);
  },
  encode(decoded: Immutable<boolean>): boolean {
    return decoded;
  },
};
export function jsonTypeBoolean(): JsonType<boolean, boolean> {
  return jsonTypeBooleanCached;
}

const jsonTypeNumberCached = {
  decode(encoded: JsonValue): number {
    return jsonExpectNumber(encoded);
  },
  encode(decoded: Immutable<number>): number {
    return decoded;
  },
};
export function jsonTypeNumber(): JsonType<number, number> {
  return jsonTypeNumberCached;
}

const jsonTypeStringCached = {
  decode(encoded: JsonValue): string {
    return jsonExpectString(encoded);
  },
  encode(decoded: Immutable<string>): string {
    return decoded;
  },
};
export function jsonTypeString(): JsonType<string, string> {
  return jsonTypeStringCached;
}

const jsonTypeStringToPubkeyCached = {
  decode(encoded: JsonValue): PublicKey {
    return new PublicKey(jsonExpectString(encoded));
  },
  encode(decoded: Immutable<PublicKey>): string {
    return String(decoded);
  },
};
export function jsonTypeStringToPubkey(): JsonType<string, PublicKey> {
  return jsonTypeStringToPubkeyCached;
}

const jsonTypeStringToBigintCached = {
  decode(encoded: JsonValue): bigint {
    return BigInt(jsonExpectString(encoded));
  },
  encode(decoded: Immutable<bigint>): string {
    return String(decoded);
  },
};
export function jsonTypeStringToBigint(): JsonType<string, bigint> {
  return jsonTypeStringToBigintCached;
}

export function jsonTypeArray<ItemEncoded extends JsonValue, ItemDecoded>(
  itemType: JsonType<ItemEncoded, ItemDecoded>,
): JsonType<Array<ItemEncoded>, Array<ItemDecoded>> {
  return {
    decode(encoded: JsonValue): Array<ItemDecoded> {
      return jsonExpectArray(encoded).map((item, index) =>
        withContext(`JSON: Decode Array[${index}] =>`, () =>
          itemType.decode(item),
        ),
      );
    },
    encode(decoded: Immutable<Array<ItemDecoded>>): Array<ItemEncoded> {
      return decoded.map((item) => itemType.encode(item));
    },
  };
}

export function jsonTypeObject<
  Shape extends { [key: string]: JsonType<any, any> },
>(
  shape: Shape,
): JsonType<
  { [K in keyof Shape]: JsonTypeEncoded<Shape[K]> },
  { [K in keyof Shape]: JsonTypeDecoded<Shape[K]> }
> {
  return {
    decode(encoded: JsonValue): {
      [K in keyof Shape]: JsonTypeDecoded<Shape[K]>;
    } {
      const object = jsonExpectObject(encoded);
      const decoded = {} as { [K in keyof Shape]: JsonTypeDecoded<Shape[K]> };
      for (const key in shape) {
        decoded[key] = withContext(`JSON: Decode Object["${key}"] =>`, () =>
          shape[key]!.decode(object[key]!),
        );
      }
      return decoded;
    },
    encode(
      decoded: Immutable<{
        [K in keyof Shape]: JsonTypeDecoded<Shape[K]>;
      }>,
    ): {
      [K in keyof Shape]: JsonTypeEncoded<Shape[K]>;
    } {
      const encoded = {} as { [K in keyof Shape]: JsonTypeEncoded<Shape[K]> };
      for (const key in shape) {
        encoded[key] = shape[key]!.encode(decoded[key as keyof typeof decoded]);
      }
      return encoded;
    },
  };
}

export function jsonTypeObjectToRecord<
  ValueEncoded extends JsonValue,
  ValueDecoded,
>(
  valueType: JsonType<ValueEncoded, ValueDecoded>,
): JsonType<Record<string, ValueEncoded>, Record<string, ValueDecoded>> {
  return {
    decode(encoded: JsonValue): Record<string, ValueDecoded> {
      const object = jsonExpectObject(encoded);
      const decoded: Record<string, ValueDecoded> = {};
      for (const key in object) {
        decoded[key] = withContext(`JSON: Decode Object["${key}"] =>`, () =>
          valueType.decode(object[key]!),
        );
      }
      return decoded;
    },
    encode(
      decoded: Immutable<Record<string, ValueDecoded>>,
    ): Record<string, ValueEncoded> {
      const encoded: Record<string, ValueEncoded> = {};
      for (const [key, value] of Object.entries(decoded)) {
        encoded[key] = valueType.encode(value);
      }
      return encoded;
    },
  };
}

export function jsonTypeObjectToMap<
  ValueEncoded extends JsonValue,
  ValueDecoded,
>(
  valueType: JsonType<ValueEncoded, ValueDecoded>,
): JsonType<Record<string, ValueEncoded>, Map<string, ValueDecoded>> {
  return {
    decode(encoded: JsonValue): Map<string, ValueDecoded> {
      const object = jsonExpectObject(encoded);
      const decoded = new Map<string, ValueDecoded>();
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
    encode(
      decoded: Immutable<Map<string, ValueDecoded>>,
    ): Record<string, ValueEncoded> {
      const encoded: Record<string, ValueEncoded> = {};
      for (const [key, val] of decoded.entries()) {
        encoded[key] = valueType.encode(val);
      }
      return encoded;
    },
  };
}

export function jsonTypeArrayToMap<
  KeyEncoded extends JsonValue,
  KeyDecoded,
  ValueEncoded extends JsonValue,
  ValueDecoded,
>(
  keyType: JsonType<KeyEncoded, KeyDecoded>,
  valueType: JsonType<ValueEncoded, ValueDecoded>,
): JsonType<Array<[KeyEncoded, ValueEncoded]>, Map<KeyDecoded, ValueDecoded>> {
  return {
    decode(encoded: JsonValue): Map<KeyDecoded, ValueDecoded> {
      const array = jsonExpectArray(encoded);
      const decoded = new Map<KeyDecoded, ValueDecoded>();
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
    encode(
      decoded: Immutable<Map<KeyDecoded, ValueDecoded>>,
    ): Array<[KeyEncoded, ValueEncoded]> {
      const encoded: Array<[KeyEncoded, ValueEncoded]> = [];
      for (const [key, val] of decoded.entries()) {
        encoded.push([keyType.encode(key), valueType.encode(val)]);
      }
      encoded.sort();
      return encoded;
    },
  };
}

export function jsonTypeNullable<
  ContentEncoded extends JsonValue,
  ContentDecoded,
>(
  contentType: JsonType<ContentEncoded, ContentDecoded>,
): JsonType<ContentEncoded | null, ContentDecoded | null> {
  return {
    decode(encoded: JsonValue): ContentDecoded | null {
      if (encoded === null) {
        return null;
      }
      return contentType.decode(encoded);
    },
    encode(decoded: Immutable<ContentDecoded | null>): ContentEncoded | null {
      if (decoded === null) {
        return null;
      }
      return contentType.encode(decoded);
    },
  };
}

export function jsonTypeNullableToOptional<
  ContentEncoded extends JsonValue,
  ContentDecoded,
>(
  contentType: JsonType<ContentEncoded, ContentDecoded>,
): JsonType<ContentEncoded | null, ContentDecoded | undefined> {
  return {
    decode(encoded: JsonValue): ContentDecoded | undefined {
      if (encoded === null) {
        return undefined;
      }
      return contentType.decode(encoded);
    },
    encode(
      decoded: Immutable<ContentDecoded | undefined>,
    ): ContentEncoded | null {
      if (decoded === undefined) {
        return null;
      }
      return contentType.encode(decoded);
    },
  };
}
