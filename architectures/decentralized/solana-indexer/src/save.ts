import { promises as fsp } from "fs";
import { dirname, join } from "path";
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
import { utilLogWithTimestamp, utilsGetStateDirectory } from "./utils";

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
  await fileWriteSafely(
    filePath(programAddress, programName, `backup_${fileNameDateOnly()}`),
    content,
  );
  await fileWriteSafely(
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
    const content = await fsp.readFile(
      filePath(programAddress, programName, fileTagLatest),
      "utf-8",
    );
    await fileWriteSafely(
      filePath(programAddress, programName, `start_${fileNameDateTime()}`),
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

async function fileWriteSafely(
  filePath: string,
  content: string,
): Promise<void> {
  const filePathTmp = `${filePath}.tmp`;
  await fsp.mkdir(dirname(filePathTmp), { recursive: true });
  await fsp.writeFile(filePathTmp, content, { flush: true });
  await fsp.mkdir(dirname(filePath), { recursive: true });
  await fsp.rename(filePathTmp, filePath);
}

function fileNameDateOnly(): string {
  const now = new Date();
  return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}`;
}

function fileNameDateTime(): string {
  const now = new Date();
  return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}_${now.getHours()}-${now.getMinutes()}-${now.getSeconds()}`;
}

const fileTagLatest = "latest";

const fileJsonCodec = jsonCodecObjectToObject({
  updatedAt: jsonCodecString,
  checkpoint: jsonCodecValue,
  dataStore: jsonCodecValue,
});
