import fs from "fs";
import { JsonValue } from "./json";
import {
  jsonTypeNumber,
  jsonTypeObject,
  jsonTypeString,
  jsonTypeValue,
} from "./jsonType";

const jsonType = jsonTypeObject({
  version: jsonTypeNumber(),
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
  const encoded = jsonType.encode({
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
  const json = await fs.promises
    .readFile(path, "utf-8")
    .then((data: string) => JSON.parse(data) as JsonValue);
  const decoded = jsonType.decode(json);
  if (decoded.version !== 1) {
    throw new Error("Unsupported version");
  }
  return {
    updatedAt: decoded.updatedAt,
    checkpoint: decoded.checkpoint,
    dataStore: decoded.dataStore,
  };
}
