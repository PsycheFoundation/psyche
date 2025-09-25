import { JsonValue } from "../json";
import { jsonTypeNumber, jsonTypeObject } from "../jsonType";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

const jsonTypeV1 = jsonTypeObject({
  version: jsonTypeNumber(),
});

export function coordinatorDataToJson(
  dataStore: CoordinatorDataStore,
): JsonValue {
  return jsonTypeV1.encode({
    version: 1,
  });
}

export function coordinatorDataFromJson(
  jsonValue: JsonValue,
): CoordinatorDataStore {
  const decoded = jsonTypeV1.decode(jsonValue);
  if (decoded.version !== 1) {
    throw new Error(`Unsupported coordinator data version`);
  }
  // TODO - use decoded
  return new CoordinatorDataStore();
}
