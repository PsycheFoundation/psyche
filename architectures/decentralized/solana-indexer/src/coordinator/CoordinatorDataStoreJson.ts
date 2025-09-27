import { jsonTypeObject, jsonTypeWrap } from "../jsonType";
import { CoordinatorDataStore } from "./CoordinatorDataStore";

export const coordinatorAccountJsonTypeV1 = jsonTypeObject({});

// TODO - implement
export const coordinatorDataStoreJsonType = jsonTypeWrap(
  coordinatorAccountJsonTypeV1,
  (decoded) => new CoordinatorDataStore(new Map()),
  (encoded) => ({}),
);

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
