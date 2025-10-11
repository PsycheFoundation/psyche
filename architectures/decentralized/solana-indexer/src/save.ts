import fs from 'fs'
import {
	jsonCodecObject,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
} from 'solana-kiss'

const saveFolder = process.env['STATE_DIRECTORY'] ?? process.cwd()

const saveJsonCodec = jsonCodecObject({
	updatedAt: jsonCodecString,
	checkpoint: jsonCodecRaw,
	dataStore: jsonCodecRaw,
})

async function savePath(saveName: string): Promise<string> {
	return `${saveFolder}/${saveName}.json`
}

export async function saveWrite(
	saveName: string,
	saveContent: {
		checkpoint: JsonValue
		dataStore: JsonValue
	}
): Promise<void> {
	const path = await savePath(saveName)
	const encoded = saveJsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	return fs.promises.writeFile(path, JSON.stringify(encoded))
}

export async function saveRead(saveName: string): Promise<{
	updatedAt: string
	checkpoint: JsonValue
	dataStore: JsonValue
}> {
	const path = await savePath(saveName)
	const encoded = await fs.promises
		.readFile(path, 'utf-8')
		.then((data: string) => JSON.parse(data) as JsonValue)
	return saveJsonCodec.decoder(encoded)
}
