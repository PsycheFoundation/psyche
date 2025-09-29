import { Immutable, withContext } from "./utils";

export type JsonValue =
  | undefined
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
  if (value === undefined) {
    return "Undefined";
  }
  if (value === null) {
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

export function jsonExpectValueIsSubset(
  subset: JsonValue,
  superset: JsonValue,
) {
  if (subset === undefined) {
    return true;
  }
  if (subset === null) {
    return superset === null;
  }
  const boolean = jsonAsBoolean(subset);
  if (boolean !== undefined) {
    return jsonAsBoolean(superset) === boolean;
  }
  const number = jsonAsNumber(subset);
  if (number !== undefined) {
    return jsonAsNumber(superset) === number;
  }
  const string = jsonAsString(subset);
  if (string !== undefined) {
    return jsonAsString(superset) === string;
  }
  const subsetArray = jsonAsArray(subset);
  if (subsetArray !== undefined) {
    const supersetArray = jsonAsArray(superset);
    if (
      supersetArray === undefined ||
      subsetArray.length > supersetArray.length
    ) {
      return false;
    }
    for (let index = 0; index < subsetArray.length; index++) {
      if (
        !jsonExpectValueIsSubset(subsetArray[index]!, supersetArray[index]!)
      ) {
        return false;
      }
    }
    return true;
  }
  const subsetObject = jsonAsObject(subset);
  if (subsetObject !== undefined) {
    const supersetObject = jsonAsObject(superset);
    if (supersetObject === undefined) {
      return false;
    }
    for (const key of Object.keys(subsetObject)) {
      if (!jsonExpectValueIsSubset(subsetObject[key]!, supersetObject[key])) {
        return false;
      }
    }
    return true;
  }
  throw new Error(`JSON: Unknown value: ${subset?.toString()}`);
}

export function jsonExpectUndefined(value: JsonValue): undefined {
  if (value !== undefined) {
    throw new Error(`JSON: Expected undefined (found: ${jsonPreview(value)})`);
  }
  return undefined;
}
export function jsonExpectNull(value: JsonValue): null {
  if (value !== null) {
    throw new Error(`JSON: Expected null (found: ${jsonPreview(value)})`);
  }
  return null;
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
    encode(decoded: Immutable<Array<Item>>): JsonValue {
      return decoded.map((item) => itemType.encode(item));
    },
  };
}

export function jsonTypeArrayToTuple<
  Items extends [JsonType<any>, ...JsonType<any>[]],
>(
  itemsTypes: Items,
): JsonType<{ [K in keyof Items]: JsonTypeContent<Items[K]> }> {
  return {
    decode(encoded: JsonValue): {
      [K in keyof Items]: JsonTypeContent<Items[K]>;
    } {
      const array = jsonExpectArray(encoded);
      if (array.length !== itemsTypes.length) {
        throw new Error(
          `JSON: Expected tuple array of length ${itemsTypes.length} (found length: ${array.length})`,
        );
      }
      const decoded = {} as { [K in keyof Items]: JsonTypeContent<Items[K]> };
      for (let index = 0; index < itemsTypes.length; index++) {
        decoded[index as keyof typeof decoded] = withContext(
          `JSON: Decode Array[${index}] =>`,
          () => itemsTypes[index]!.decode(array[index]!),
        ) as JsonTypeContent<Items[typeof index]>;
      }
      return decoded;
    },
    encode(
      decoded: Immutable<{ [K in keyof Items]: JsonTypeContent<Items[K]> }>,
    ): JsonValue {
      const array = new Array<JsonValue>();
      for (let index = 0; index < itemsTypes.length; index++) {
        array.push(
          itemsTypes[index]!.encode(decoded[index as keyof typeof decoded]),
        );
      }
      return array;
    },
  };
}

export function jsonTypeObject<Shape extends { [key: string]: JsonType<any> }>(
  shape: Shape,
  keysEncoding?: { [K in keyof Shape]?: string },
): JsonType<{ [K in keyof Shape]: JsonTypeContent<Shape[K]> }> {
  return {
    decode(encoded: JsonValue): {
      [K in keyof Shape]: JsonTypeContent<Shape[K]>;
    } {
      const object = jsonExpectObject(encoded);
      const decoded = {} as { [K in keyof Shape]: JsonTypeContent<Shape[K]> };
      for (const keyDecoded in shape) {
        const keyEncoded = keysEncoding?.[keyDecoded] ?? keyDecoded;
        decoded[keyDecoded] = withContext(
          `JSON: Decode Object["${keyEncoded}"] =>`,
          () => shape[keyDecoded]!.decode(object[keyEncoded]),
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
        const keyEncoded = keysEncoding?.[keyDecoded] ?? keyDecoded;
        encoded[keyEncoded] = shape[keyDecoded]!.encode(
          decoded[keyDecoded as keyof typeof decoded],
        );
      }
      return encoded;
    },
  };
}

export function jsonTypeObjectWithKeyEncoder<
  Shape extends { [key: string]: JsonType<any> },
>(shape: Shape, keyEncoder: (key: string) => string) {
  const keysEncoding = {} as { [K in keyof Shape]: string };
  for (const keyDecoded in shape) {
    keysEncoding[keyDecoded] = keyEncoder(keyDecoded);
  }
  return jsonTypeObject(shape, keysEncoding);
}

export function jsonTypeObjectToRecord<Value>(
  valueType: JsonType<Value>,
): JsonType<Record<string, Value>> {
  return {
    decode(encoded: JsonValue): Record<string, Value> {
      const object = jsonExpectObject(encoded);
      const decoded: Record<string, Value> = {};
      for (const key of Object.keys(object)) {
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
      for (const key of Object.keys(object)) {
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
      for (let index = 0; index < array.length; index++) {
        const item = array[index]!;
        const keyValue = jsonExpectArray(item);
        if (keyValue.length !== 2) {
          throw new Error(`JSON: Expected key-value array of length 2`);
        }
        decoded.set(
          withContext(`JSON: Decode Array[${index}]["key"] =>`, () =>
            keyType.decode(keyValue[0]!),
          ),
          withContext(`JSON: Decode Array[${index}]["value"] =>`, () =>
            valueType.decode(keyValue[1]!),
          ),
        );
      }
      return decoded;
    },
    encode(decoded: Immutable<Map<Key, Value>>): JsonValue {
      const encoded = new Array<JsonValue>();
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
      if (encoded === null || encoded === undefined) {
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

export function jsonTypeOptional<Content>(
  contentType: JsonType<Content>,
): JsonType<Content | undefined> {
  return {
    decode(encoded: JsonValue): Content | undefined {
      if (encoded === null || encoded === undefined) {
        return undefined;
      }
      return contentType.decode(encoded);
    },
    encode(decoded: Immutable<Content | undefined>): JsonValue {
      if (decoded === undefined) {
        return undefined;
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

export function jsonTypeArrayToVariant<Variant>(
  variantKey: string,
  variantType: JsonType<Variant>,
): JsonType<Variant> {
  return jsonTypeMapped(
    jsonTypeArrayToTuple([jsonTypeConst(variantKey), variantType]),
    {
      map: (unmapped) => unmapped[1]!,
      unmap: (mapped) => [variantKey, mapped] as [string, Immutable<Variant>],
    },
  );
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
      const errors = new Array();
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
