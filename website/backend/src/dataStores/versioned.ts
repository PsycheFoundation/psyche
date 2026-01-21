import { openSync, readSync, closeSync, readFileSync } from "fs";
import { CURRENT_VERSION, Version, formats } from "shared";
import { writeFileAtomic } from "../writeFileAtomic.js";

export function readVersionedFile(path: string): {
  version: Version;
  data: any;
} {
  // Read first max 200 bytes to find the newline
  const fd = openSync(path, "r");
  const buffer = Buffer.alloc(200);
  const bytesRead = readSync(fd, buffer, 0, 200, 0);

  const firstChunk = buffer.subarray(0, bytesRead).toString("utf-8");
  const newlineIndex = firstChunk.indexOf("\n");

  let version: Version = "unversioned";
  if (newlineIndex === -1) {
    closeSync(fd);
    console.warn(
      "No format version found in the first 200 chars. assuming it's unversioned (init format).",
    );
  } else {
    const firstLine = firstChunk.substring(0, newlineIndex);
    version = JSON.parse(firstLine);
  }
  if (!(version in formats)) {
    throw new Error(
      `Invalid version ${version} in file. Expected one of ${Object.keys(
        formats,
      )
        .filter((v) => v !== "unversioned")
        .join(", ")}`,
    );
  }
  const restOfFile = readFileSync(path, "utf-8").substring(newlineIndex);
  console.log(`Loading file with version ${version}...`);
  const data = JSON.parse(restOfFile, formats[version].reviver);
  return { version, data };
}

export async function writeVersionedFile(
  path: string,
  data: any,
): Promise<void> {
  await writeFileAtomic(
    path,
    `${CURRENT_VERSION}\n` +
      JSON.stringify(data, formats[CURRENT_VERSION].replacer),
  );
}
