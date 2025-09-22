export async function fileJsonWrite(path: string, json: any): Promise<void> {
  const fs = require("fs").promises;
  return fs.writeFile(path, JSON.stringify(json, null, 2));
}

export async function fileJsonRead(path: string): Promise<any> {
  const fs = require("fs").promises;
  return fs.readFile(path, "utf-8").then((data: string) => JSON.parse(data));
}
