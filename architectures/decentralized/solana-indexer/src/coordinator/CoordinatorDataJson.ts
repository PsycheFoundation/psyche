import { JsonValue } from "../json";
import { jsonTypeObject } from "../jsonType";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export const coordinatorAccountJsonTypeV1 = jsonTypeObject({});

export function coordinatorDataToJson(
  dataStore: CoordinatorDataStore,
): JsonValue {
  return coordinatorAccountJsonTypeV1.encode({});
}

export function coordinatorDataFromJson(
  jsonValue: JsonValue,
): CoordinatorDataStore {
  console.log("Decoding coordinator data from JSON", jsonValue);
  //const decoded = coordinatorAccountJsonTypeV1.decode(jsonValue);
  return new CoordinatorDataStore(new Map());
}

/*
function jsonTypeWrappedArray<Item>(
  itemType: JsonType<Item>,
): JsonType<Array<Item>> {
  return {
    decode(encoded: JsonValue): Array<Item> {
      const arrayOuter = jsonExpectObject(encoded);
      const arrayInner = jsonExpectArray(
        jsonExpectValueFromArray(arrayOuter, 0),
      );
      return arrayOuter.map((item, index) =>
        withContext(`JSON: Decode Array[${index}] =>`, () =>
          elementType.decode(item),
        ),
      ) as readonly ElementDecoded[] & { length: Length };
    },
    encode(decoded: Immutable<Array<Item>>): JsonValue {
      if (decoded.length !== length) {
        throw new Error(
          `Expected array of length ${length}, got ${decoded.length}`,
        );
      }
      return decoded.map((item, index) =>
        withContext(`JSON: Encode Array[${index}] =>`, () =>
          elementType.encode(item),
        ),
      );
    },
  };
}
*/
