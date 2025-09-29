import fs from "fs";
import {
  jsonTypeObject,
  jsonTypeObjectToVariant,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
} from "./json";

const saveJsonType = jsonTypeObjectToVariant(
  "save_v1",
  jsonTypeObject({
    updatedAt: jsonTypeString(),
    checkpoint: jsonTypeValue(),
    dataStore: jsonTypeValue(),
  }),
);

async function savePath(saveName: string): Promise<string> {
  // TODO - env variable for data directory
  return `./${saveName}.json`;
}

export async function saveWrite(
  saveName: string,
  saveContent: {
    checkpoint: JsonValue;
    dataStore: JsonValue;
  },
): Promise<void> {
  const path = await savePath(saveName);
  const encoded = saveJsonType.encode({
    updatedAt: new Date().toISOString(),
    checkpoint: saveContent.checkpoint,
    dataStore: saveContent.dataStore,
  });
  return fs.promises.writeFile(path, JSON.stringify(encoded, null, 2));
}

export async function saveRead(saveName: string): Promise<{
  updatedAt: string;
  checkpoint: JsonValue;
  dataStore: JsonValue;
}> {
  const path = await savePath(saveName);
  const encoded = await fs.promises
    .readFile(path, "utf-8")
    .then((data: string) => JSON.parse(data) as JsonValue);
  return saveJsonType.decode(encoded);
}
