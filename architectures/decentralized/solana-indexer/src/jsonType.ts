import {
  jsonAsArray,
  jsonAsObject,
  jsonExpectArray,
  jsonExpectBoolean,
  jsonExpectNull,
  jsonExpectNumber,
  jsonExpectObject,
  jsonExpectString,
  jsonIsBoolean,
  jsonIsNull,
  jsonIsNumber,
  jsonIsString,
  JsonValue,
} from "./json";
import { Immutable, withContext } from "./main";

export type JsonTypeEncoded<S> = S extends JsonType<infer T, any> ? T : never;
export type JsonTypeDecoded<S> = S extends JsonType<any, infer T> ? T : never;

export type JsonType<Encoded extends JsonValue, Decoded> = {
  validate(encoded: JsonValue): boolean;
  decode(encoded: JsonValue): Decoded;
  encode(decoded: Immutable<Decoded>): Encoded;
};

export function jsonTypeValue(): JsonType<JsonValue, JsonValue> {
  // TODO - cache
  return {
    validate(): boolean {
      return true;
    },
    decode(encoded: JsonValue): JsonValue {
      return encoded;
    },
    encode(decoded: Immutable<JsonValue>): JsonValue {
      return JSON.parse(JSON.stringify(decoded));
    },
  };
}
export function jsonTypeNull(): JsonType<null, null> {
  // TODO - cache
  return {
    validate(encoded: JsonValue): boolean {
      return jsonIsNull(encoded);
    },
    decode(encoded: JsonValue): null {
      return jsonExpectNull(encoded);
    },
    encode(decoded: Immutable<null>): null {
      return decoded;
    },
  };
}
export function jsonTypeBoolean(): JsonType<boolean, boolean> {
  // TODO - cache
  return {
    validate(encoded: JsonValue): boolean {
      return jsonIsBoolean(encoded);
    },
    decode(encoded: JsonValue): boolean {
      return jsonExpectBoolean(encoded);
    },
    encode(decoded: Immutable<boolean>): boolean {
      return decoded;
    },
  };
}
export function jsonTypeNumber(): JsonType<number, number> {
  // TODO - cache
  return {
    validate(encoded: JsonValue): boolean {
      return jsonIsNumber(encoded);
    },
    decode(encoded: JsonValue): number {
      return jsonExpectNumber(encoded);
    },
    encode(decoded: Immutable<number>): number {
      return decoded;
    },
  };
}

// TODO - support const numbers/strings/booleans

export function jsonTypeString(): JsonType<string, string> {
  // TODO - cache
  return {
    validate(encoded: JsonValue): boolean {
      return jsonIsString(encoded);
    },
    decode(encoded: JsonValue): string {
      return jsonExpectString(encoded);
    },
    encode(decoded: Immutable<string>): string {
      return decoded;
    },
  };
}
export function jsonTypeStringToBigint(): JsonType<string, bigint> {
  // TODO - cache
  return {
    validate(encoded: JsonValue): boolean {
      return jsonIsString(encoded);
    },
    decode(encoded: JsonValue): bigint {
      return BigInt(jsonExpectString(encoded));
    },
    encode(decoded: Immutable<bigint>): string {
      return String(decoded);
    },
  };
}

export function jsonTypeArray<ItemEncoded extends JsonValue, ItemDecoded>(
  itemType: JsonType<ItemEncoded, ItemDecoded>,
): JsonType<Array<ItemEncoded>, Array<ItemDecoded>> {
  return {
    validate(encoded: JsonValue): boolean {
      const array = jsonAsArray(encoded);
      if (array === undefined) {
        return false;
      }
      return array.every((item) => itemType.validate(item));
    },
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
    validate(encoded: JsonValue): boolean {
      const object = jsonAsObject(encoded);
      if (object === undefined) {
        return false;
      }
      for (const key in shape) {
        if (!shape[key]!.validate(object[key]!)) {
          return false;
        }
      }
      return true;
    },
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
    validate(encoded: JsonValue): boolean {
      const object = jsonAsObject(encoded);
      if (object === undefined) {
        return false;
      }
      for (const key in object) {
        if (!valueType.validate(object[key]!)) {
          return false;
        }
      }
      return true;
    },
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
    validate(encoded: JsonValue): boolean {
      const object = jsonAsObject(encoded);
      if (object === undefined) {
        return false;
      }
      for (const key in object) {
        if (!valueType.validate(object[key]!)) {
          return false;
        }
      }
      return true;
    },
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

export function jsonTypeNullable<
  ContentEncoded extends JsonValue,
  ContentDecoded,
>(
  contentType: JsonType<ContentEncoded, ContentDecoded>,
): JsonType<ContentEncoded | null, ContentDecoded | null> {
  return {
    validate(encoded: JsonValue): boolean {
      return encoded === null || contentType.validate(encoded);
    },
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
    validate(encoded: JsonValue): boolean {
      return encoded === null || contentType.validate(encoded);
    },
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
    validate(encoded: JsonValue): boolean {
      const array = jsonAsArray(encoded);
      if (array === undefined) {
        return false;
      }
      for (const item of array) {
        const keyValue = jsonAsArray(item);
        if (keyValue === undefined || keyValue.length !== 2) {
          return false;
        }
        if (!keyType.validate(keyValue[0]!)) {
          return false;
        }
        if (!valueType.validate(keyValue[1]!)) {
          return false;
        }
      }
      return true;
    },
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
      return encoded;
    },
  };
}
