import { jsonSchemaNumber, jsonSchemaObject, JsonValue } from "../json";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

const coordinatorJsonSchema = jsonSchemaObject({
  version: jsonSchemaNumber(),
});

export function coordinatorDataToJson(
  dataStore: CoordinatorDataStore,
): JsonValue {
  return coordinatorJsonSchema.guard({
    version: 1,
  });
}

export function coordinatorDataFromJson(
  jsonValue: JsonValue,
): CoordinatorDataStore {
  const typedValue = coordinatorJsonSchema.parse(jsonValue);
  if (typedValue.version !== 1) {
    throw new Error("Unsupported version");
  }
  return new CoordinatorDataStore();
}
