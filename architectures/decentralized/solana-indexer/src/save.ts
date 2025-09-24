import fs from "fs";
import {
  jsonSchemaNumberConst,
  jsonSchemaObject,
  jsonSchemaString,
  jsonSchemaValue,
  JsonValue,
} from "./json";

const jsonSchema = jsonSchemaObject({
  version: jsonSchemaNumberConst(1),
  updatedAt: jsonSchemaString(),
  checkpoint: jsonSchemaValue(),
  dataStore: jsonSchemaValue(),
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
  const json = jsonSchema.guard({
    version: 1,
    updatedAt: saveContent.updatedAt,
    checkpoint: saveContent.checkpoint,
    dataStore: saveContent.dataStore,
  });
  return fs.promises.writeFile(path, JSON.stringify(json, null, 2));
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
  const parsed = jsonSchema.parse(json);
  if (parsed.version !== 1) {
    throw new Error("Unsupported version");
  }
  return {
    updatedAt: parsed.updatedAt,
    checkpoint: parsed.checkpoint,
    dataStore: parsed.dataStore,
  };
}
