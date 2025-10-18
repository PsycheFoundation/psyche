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
	const encoded = jsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	const startTime = Date.now()
	await fs.promises.writeFile(path, JSON.stringify(encoded))
	console.log(
		new Date().toISOString(),
		'>>>',
		`Saved ${saveName} in ${Date.now() - startTime}ms`
	)
}

export async function saveRead(saveName: string): Promise<{
	updatedAt: string
	checkpoint: JsonValue
	dataStore: JsonValue
}> {
	const path = savePath(saveName)
	const encoded = await fs.promises
		.readFile(path, 'utf-8')
		.then((data: string) => JSON.parse(data) as JsonValue)
	return jsonCodec.decoder(encoded)
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
