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
