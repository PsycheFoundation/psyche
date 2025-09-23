import fs from "fs";
import { JsonValue } from "./json";

export async function fileJsonPath(file: string): Promise<string> {
  // TODO - env variable for data directory
  return `./${file}`;
}

export async function fileJsonWrite(
  file: string,
  json: JsonValue,
): Promise<void> {
  const path = await fileJsonPath(file);
  return fs.promises.writeFile(path, JSON.stringify(json, null, 2));
}

export async function fileJsonRead(file: string): Promise<JsonValue> {
  const path = await fileJsonPath(file);
  return fs.promises
    .readFile(path, "utf-8")
    .then((data: string) => JSON.parse(data));
}
