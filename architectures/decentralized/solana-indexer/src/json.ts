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
  if (jsonIsNull(value)) {
    return "Null";
  }
  if (jsonIsBoolean(value)) {
    return `Boolean: ${value}`;
  }
  if (jsonIsNumber(value)) {
    return `Number: ${value}`;
  }
  if (jsonIsString(value)) {
    return `String: "${value}"`;
  }
  const maxColumns = 40;
  if (jsonIsArray(value)) {
    const array = value as JsonArray;
    let previews = array.map(jsonPreview).join(", ");
    if (previews.length > maxColumns) {
      previews = previews.slice(0, maxColumns - 3) + "...";
    }
    return `Array(x${array.length}): [${previews}]`;
  }
  if (jsonIsObject(value)) {
    const object = value as JsonObject;
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

export function jsonIsNull(value: JsonValue): boolean {
  return jsonAsNull(value) !== undefined;
}
export function jsonIsBoolean(value: JsonValue): boolean {
  return jsonAsBoolean(value) !== undefined;
}
export function jsonIsNumber(value: JsonValue): boolean {
  return jsonAsNumber(value) !== undefined;
}
export function jsonIsString(value: JsonValue): boolean {
  return jsonAsString(value) !== undefined;
}
export function jsonIsArray(value: JsonValue): boolean {
  return jsonAsArray(value) !== undefined;
}
export function jsonIsObject(value: JsonValue): boolean {
  return jsonAsObject(value) !== undefined;
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

export type JsonSchemaInfered<S> = S extends JsonSchema<infer T> ? T : never;
export type JsonSchema<T> = {
  check: (value: JsonValue) => boolean;
  parse: (value: JsonValue) => T;
  guard: (value: T) => JsonValue;
};

function jsonSchemaGuard<T>(value: T) {
  return value;
}

const jsonSchemaValueCached = {
  check: (_value: JsonValue) => true,
  parse: (value: JsonValue) => value,
  guard: jsonSchemaGuard<JsonValue>,
};
export function jsonSchemaValue() {
  return jsonSchemaValueCached;
}

const jsonSchemaNullCached = {
  check: (value: JsonValue) => jsonIsNull(value),
  parse: (value: JsonValue) => jsonExpectNull(value),
  guard: jsonSchemaGuard<null>,
};
export function jsonSchemaNull() {
  return jsonSchemaNullCached;
}

const jsonSchemaBooleanCached = {
  check: (value: JsonValue) => jsonIsBoolean(value),
  parse: (value: JsonValue) => jsonExpectBoolean(value),
  guard: jsonSchemaGuard<boolean>,
};
export function jsonSchemaBoolean() {
  return jsonSchemaBooleanCached;
}

const jsonSchemaNumberCached = {
  check: (value: JsonValue) => jsonIsNumber(value),
  parse: (value: JsonValue) => jsonExpectNumber(value),
  guard: jsonSchemaGuard<number>,
};
export function jsonSchemaNumber() {
  return jsonSchemaNumberCached;
}

const jsonSchemaStringCached = {
  check: (value: JsonValue) => jsonIsString(value),
  parse: (value: JsonValue) => jsonExpectString(value),
  guard: jsonSchemaGuard<string>,
};
export function jsonSchemaString() {
  return jsonSchemaStringCached;
}

export function jsonSchemaArray<T>(items: JsonSchema<T>) {
  return {
    items,
    check: (value: JsonValue) => {
      const array = jsonAsArray(value);
      if (array === undefined) {
        return false;
      }
      return array.every((value) => items.check(value));
    },
    parse: (value: JsonValue) => jsonExpectArray(value).map(items.parse),
    guard: jsonSchemaGuard<T[]>,
  } as JsonSchema<T[]>;
}

export function jsonSchemaTuple<T extends JsonSchema<any>[]>(...items: T) {
  return {
    check: (value: JsonValue) => {
      const array = jsonAsArray(value);
      if (array === undefined) {
        return false;
      }
      if (array.length !== items.length) {
        return false;
      }
      return array.every((v, i) => items[i]!.check(v));
    },
    parse: (value: JsonValue) => {
      const array = jsonExpectArray(value);
      if (array.length !== items.length) {
        throw new Error(
          `JSON: Expected tuple of length ${items.length} (found: ${array.length})`,
        );
      }
      return array.map((v, i) => items[i]!.parse(v)) as {
        [K in keyof T]: T[K] extends JsonSchema<infer U> ? U : never;
      };
    },
    guard: jsonSchemaGuard<{
      [K in keyof T]: T[K] extends JsonSchema<infer U> ? U : never;
    }>,
  };
}

export function jsonSchemaObject<S extends Record<string, JsonSchema<any>>>(
  shape: S,
) {
  return {
    check: (value: JsonValue) => {
      const object = jsonAsObject(value);
      if (object === undefined) {
        return false;
      }
      for (const key in shape) {
        const value = object[key];
        if (value === undefined) {
          return false;
        }
        if (!shape[key]!.check(value)) {
          return false;
        }
      }
      return true;
    },
    parse: (value: JsonValue) => {
      const object = jsonExpectObject(value);
      const result = {} as { [K in keyof S]: JsonSchemaInfered<S[K]> };
      for (const key in shape) {
        result[key as keyof S] = shape[key]!.parse(
          jsonExpectValueFromObject(object, key),
        );
      }
      return result;
    },
    guard: jsonSchemaGuard<{ [K in keyof S]: JsonSchemaInfered<S[K]> }>,
  };
}

export function jsonSchemaRecord<T>(values: JsonSchema<T>) {
  return {
    values,
    check: (value: JsonValue) => {
      const object = jsonAsObject(value);
      if (object === undefined) {
        return false;
      }
      for (const key in object) {
        if (!values.check(object[key]!)) {
          return false;
        }
      }
      return true;
    },
    parse: (value: JsonValue) => {
      const object = jsonExpectObject(value);
      const result = {} as { [key: string]: T };
      for (const key in object) {
        result[key] = values.parse(object[key]!);
      }
      return result;
    },
    guard: jsonSchemaGuard<{ [key: string]: T }>,
  } as JsonSchema<{ [key: string]: T }>;
}

export function jsonSchemaUnion<S extends JsonSchema<any>[]>(...schemas: S) {
  return {
    check: (value: JsonValue): value is JsonSchemaInfered<S[number]> => {
      return schemas.some((schema) => schema.check(value));
    },
    parse: (value: JsonValue): JsonSchemaInfered<S[number]> => {
      const errors: string[] = [];
      for (const schema of schemas) {
        if (schema.check(value)) {
          try {
            return schema.parse(value) as JsonSchemaInfered<S[number]>;
          } catch (error) {
            errors.push(String(error));
          }
        }
      }
      throw new Error(`No union variant matched:\n- ${errors.join("\n- ")}`);
    },
    guard: jsonSchemaGuard<JsonSchemaInfered<S[number]>>,
  } as JsonSchema<JsonSchemaInfered<S[number]>>;
}
