import { join } from "path";
import {
  jsonCodecObjectToObject,
  jsonCodecString,
  jsonCodecValue,
  JsonDecoder,
  JsonEncoder,
  JsonValue,
  Pubkey,
  pubkeyToBase58,
} from "solana-kiss";
import {
  CrawlerCheckpoint,
  crawlerCheckpointJsonCodec,
} from "./crawler/CrawlerTypes";
import {
  utilLogWithTimestamp,
  utilsFsRead,
  utilsFsWrite,
  utilsGetStateDirectory,
} from "./utils";

export async function saveWrite<DataStore>(
  programAddress: Pubkey,
  programName: string,
  checkpoint: CrawlerCheckpoint,
  dataStore: DataStore,
  dataStoreJsonEncoder: JsonEncoder<DataStore>,
): Promise<void> {
  const startTime = Date.now();
  const content = JSON.stringify(
    fileJsonCodec.encoder({
      updatedAt: new Date().toISOString(),
      checkpoint: crawlerCheckpointJsonCodec.encoder(checkpoint),
      dataStore: dataStoreJsonEncoder(dataStore),
    }),
  );
  await utilsFsWrite(
    filePath(programAddress, programName, `${fileNameDateOnly()}_backup`),
    content,
  );
  await utilsFsWrite(
    filePath(programAddress, programName, fileTagLatest),
    content,
  );
  utilLogWithTimestamp(
    programAddress,
    `Saved ${programName}`,
    Date.now() - startTime,
  );
}

export async function saveRead<DataStore>(
  programAddress: Pubkey,
  programName: string,
  dataStoreJsonDecoder: JsonDecoder<DataStore>,
  dataStoreFactory: () => DataStore,
): Promise<{
  updatedAt: string;
  checkpoint: CrawlerCheckpoint;
  dataStore: DataStore;
}> {
  const startTime = Date.now();
  try {
    const content = await utilsFsRead(
      filePath(programAddress, programName, fileTagLatest),
    );
    await utilsFsWrite(
      filePath(programAddress, programName, `${fileNameDateTime()}_start`),
      content,
    );
    const saveContent = fileJsonCodec.decoder(JSON.parse(content) as JsonValue);
    utilLogWithTimestamp(
      programAddress,
      `Read ${programName}`,
      Date.now() - startTime,
    );
    utilLogWithTimestamp(
      programAddress,
      `Loaded ${programName} state from: ${saveContent.updatedAt}`,
      Date.now() - startTime,
    );
    return {
      updatedAt: saveContent.updatedAt,
      checkpoint: crawlerCheckpointJsonCodec.decoder(saveContent.checkpoint),
      dataStore: dataStoreJsonDecoder(saveContent.dataStore),
    };
  } catch (error) {
    console.warn(
      `Failed to read existing ${programName} JSON, starting fresh`,
      error,
    );
    return {
      updatedAt: new Date().toISOString(),
      checkpoint: [],
      dataStore: dataStoreFactory(),
    };
  }
}

function filePath(
  saveProgramAddress: Pubkey,
  saveName: string,
  tag: string,
): string {
  return join(
    utilsGetStateDirectory(),
    "saves",
    pubkeyToBase58(saveProgramAddress),
    `${saveName}.${tag}.json`,
  );
}

function fileNameDateOnly(): string {
  const now = new Date();
  return [
    now.getFullYear().toString().padStart(4, "0"),
    (now.getMonth() + 1).toString().padStart(2, "0"),
    now.getDate().toString().padStart(2, "0"),
  ].join("-");
}

function fileNameDateTime(): string {
  const now = new Date();
  return (
    fileNameDateOnly() +
    "_" +
    [
      now.getHours().toString().padStart(2, "0"),
      now.getMinutes().toString().padStart(2, "0"),
      now.getSeconds().toString().padStart(2, "0"),
    ].join("-")
  );
}

const fileTagLatest = "current-v1";

const fileJsonCodec = jsonCodecObjectToObject({
  updatedAt: jsonCodecString,
  checkpoint: jsonCodecValue,
  dataStore: jsonCodecValue,
});
