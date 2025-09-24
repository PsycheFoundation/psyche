import {
  jsonAsArray,
  jsonExpectArray,
  jsonExpectBoolean,
  jsonExpectNull,
  jsonExpectNumber,
  jsonExpectString,
  jsonIsBoolean,
  jsonIsNull,
  jsonIsNumber,
  jsonIsString,
  JsonValue,
} from "./json";
import { withContext } from "./main";

export type JsonType<Encoded extends JsonValue, Decoded> = {
  check(json: JsonValue): boolean;
  decode(json: JsonValue): Decoded;
  encode(value: Decoded): Encoded;
};

export function jsonTypeNull(): JsonType<null, null> {
  // TODO - cache
  return {
    check(json: JsonValue): boolean {
      return jsonIsNull(json);
    },
    decode(json: JsonValue): null {
      return jsonExpectNull(json);
    },
    encode(value: null): null {
      return value;
    },
  };
}
export function jsonTypeBoolean(): JsonType<boolean, boolean> {
  // TODO - cache
  return {
    check(json: JsonValue): boolean {
      return jsonIsBoolean(json);
    },
    decode(json: JsonValue): boolean {
      return jsonExpectBoolean(json);
    },
    encode(value: boolean): boolean {
      return value;
    },
  };
}
export function jsonTypeNumber(): JsonType<number, number> {
  // TODO - cache
  return {
    check(json: JsonValue): boolean {
      return jsonIsNumber(json);
    },
    decode(json: JsonValue): number {
      return jsonExpectNumber(json);
    },
    encode(value: number): number {
      return value;
    },
  };
}
export function jsonTypeString(): JsonType<string, string> {
  // TODO - cache
  return {
    check(json: JsonValue): boolean {
      return jsonIsString(json);
    },
    decode(json: JsonValue): string {
      return jsonExpectString(json);
    },
    encode(value: string): string {
      return value;
    },
  };
}
export function jsonTypeStringToBigint(): JsonType<string, bigint> {
  // TODO - cache
  return {
    check(json: JsonValue): boolean {
      return jsonIsString(json);
    },
    decode(json: JsonValue): bigint {
      return BigInt(jsonExpectString(json));
    },
    encode(value: bigint): string {
      return String(value);
    },
  };
}

export function jsonTypeArray<ItemEncoded extends JsonValue, ItemDecoded>(
  itemType: JsonType<ItemEncoded, ItemDecoded>,
): JsonType<Array<ItemEncoded>, Array<ItemDecoded>> {
  return {
    check(json: JsonValue): boolean {
      const array = jsonAsArray(json);
      if (array === undefined) {
        return false;
      }
      return array.every((item) => itemType.check(item));
    },
    decode(json: JsonValue): Array<ItemDecoded> {
      return jsonExpectArray(json).map((item, index) =>
        withContext(`JSON: Parsing Array[${index}] =>`, () =>
          itemType.decode(item),
        ),
      );
    },
    encode(value: Array<ItemDecoded>): Array<ItemEncoded> {
      return value.map((item) => itemType.encode(item));
    },
  };
}
