import { CoordinatorDataStore } from "./CoordinatorDataStore";

export function coordinatorDataToJson(dataStore: CoordinatorDataStore): any {
  return {
    version: 1,
  };
}

export function coordinatorDataFromJson(json: any): CoordinatorDataStore {
  if (json?.["version"] !== 1) {
    throw new Error("Unsupported version");
  }
  return new CoordinatorDataStore();
}
