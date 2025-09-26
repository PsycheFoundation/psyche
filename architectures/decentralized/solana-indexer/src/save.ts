import fs from "fs";
import { JsonValue } from "./json";
import {
  jsonTypeConst,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeValue,
} from "./jsonType";

const saveJsonType = jsonTypeObject({
  version: jsonTypeConst(1),
  updatedAt: jsonTypeString(),
  checkpoint: jsonTypeValue(),
  dataStore: jsonTypeValue(),
});

async function savePath(saveName: string): Promise<string> {
  // TODO - env variable for data directory
  return `./${saveName}.json`;
}

export async function saveWrite(
  saveName: string,
  saveContent: {
    updatedAt: string;
    checkpoint: JsonValue;
    dataStore: JsonValue;
  },
): Promise<void> {
  const path = await savePath(saveName);
  const encoded = saveJsonType.encode({
    version: 1,
    updatedAt: saveContent.updatedAt,
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
  const decoded = saveJsonType.decode(encoded);
  return {
    updatedAt: decoded.updatedAt,
    checkpoint: decoded.checkpoint,
    dataStore: decoded.dataStore,
  };
}
