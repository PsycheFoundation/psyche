import { jsonSchemaNumberConst, jsonSchemaObject, JsonValue } from "../json";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

const jsonSchemaV1 = jsonSchemaObject({
  version: jsonSchemaNumberConst(1),
});

export function coordinatorDataToJson(
  dataStore: CoordinatorDataStore,
): JsonValue {
  return jsonSchemaV1.guard({
    version: 1,
  });
}

export function coordinatorDataFromJson(
  jsonValue: JsonValue,
): CoordinatorDataStore {
  const typedValue = jsonSchemaV1.parse(jsonValue);
  // TODO - use typedValue
  return new CoordinatorDataStore();
}
