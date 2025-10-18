import fs from 'fs'
import {
	jsonCodecObject,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
} from 'solana-kiss'

export async function saveWrite(
	saveName: string,
	saveContent: {
		checkpoint: JsonValue
		dataStore: JsonValue
	}
): Promise<void> {
	const path = savePath(saveName)
	const startTime = Date.now()
	const pathTemp = `${path}.${new Date().toISOString()}.tmp`
	const encoded = jsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	await fs.promises.writeFile(pathTemp, JSON.stringify(encoded))
	await fs.promises.rename(pathTemp, path)
	console.log(
		new Date().toISOString(),
		'>>>',
		`Written ${saveName} in ${Date.now() - startTime}ms`
	)
}

export async function saveRead(saveName: string): Promise<{
	updatedAt: string
	checkpoint: JsonValue
	dataStore: JsonValue
}> {
	const path = savePath(saveName)
	const startTime = Date.now()
	const encoded = await fs.promises
		.readFile(path, 'utf-8')
		.then((data: string) => JSON.parse(data) as JsonValue)
	const decoded = jsonCodec.decoder(encoded)
	console.log(
		new Date().toISOString(),
		`Read ${saveName} in ${Date.now() - startTime}ms`
	)
	return decoded
}

function savePath(saveName: string) {
	const directory = process.env['STATE_DIRECTORY'] ?? process.cwd()
	return `${directory}/${saveName}.json`
}

const jsonCodec = jsonCodecObject({
	updatedAt: jsonCodecString,
	checkpoint: jsonCodecRaw,
	dataStore: jsonCodecRaw,
})
