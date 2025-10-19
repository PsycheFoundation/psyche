import { promises as fsp } from 'fs'
import { dirname, join } from 'path'
import {
	jsonCodecObject,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
} from 'solana-kiss'
import { utilsGetStateDirectory } from './utils'

export async function saveWrite(
	saveName: string,
	saveContent: {
		checkpoint: JsonValue
		dataStore: JsonValue
	}
): Promise<void> {
	const startTime = Date.now()
	const path = savePath(saveName)
	const pathTemp = `${path}.${new Date().getTime()}.tmp`
	const encoded = jsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	await fsp.mkdir(dirname(pathTemp), { recursive: true })
	await fsp.writeFile(pathTemp, JSON.stringify(encoded), { flush: true })
	await fsp.mkdir(dirname(path), { recursive: true })
	await fsp.rename(pathTemp, path)
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
	const startTime = Date.now()
	const path = savePath(saveName)
	const encoded = await fsp
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
	return join(utilsGetStateDirectory(), 'saves', `${saveName}.json`)
}

const jsonCodec = jsonCodecObject({
	updatedAt: jsonCodecString,
	checkpoint: jsonCodecRaw,
	dataStore: jsonCodecRaw,
})
