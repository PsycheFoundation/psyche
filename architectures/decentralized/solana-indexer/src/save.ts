import fs from "fs";
import {
  jsonTypeObject,
  jsonTypeString,
  jsonTypeValue,
  JsonValue,
} from "solana-kiss";

const saveFolder = process.env["SAVE_FOLDER"] ?? ".";

const saveJsonType = jsonTypeObject((key) => key, {
  updatedAt: jsonTypeString,
  checkpoint: jsonTypeValue,
  dataStore: jsonTypeValue,
});

async function savePath(saveName: string): Promise<string> {
  return `${saveFolder}/${saveName}.json`;
}

export async function saveWrite(
  saveName: string,
  saveContent: {
    checkpoint: JsonValue;
    dataStore: JsonValue;
  },
): Promise<void> {
  const path = await savePath(saveName);
  const encoded = saveJsonType.encoder({
    updatedAt: new Date().toISOString(),
    checkpoint: saveContent.checkpoint,
    dataStore: saveContent.dataStore,
  });
  return fs.promises.writeFile(path, JSON.stringify(encoded));
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
  return saveJsonType.decoder(encoded);
}
