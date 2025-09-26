import { JsonValue } from "../json";
import { jsonTypeConst, jsonTypeObject } from "../jsonType";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

const jsonTypeV1 = jsonTypeObject({
  version: jsonTypeConst(1),
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
  return new CoordinatorDataStore();
}
