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
	const pathTemp = `${path}.${saveDateTime()}.tmp`
	const encoded = jsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	const content = JSON.stringify(encoded)
	await fsp.mkdir(dirname(pathTemp), { recursive: true })
	await fsp.writeFile(pathTemp, content, { flush: true })
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
	const pathBackup = `${path}.${saveDateTime()}.backup`
	const content = await fsp.readFile(path, 'utf-8')
	await fsp.writeFile(pathBackup, content, { flush: true })
	const encoded = JSON.parse(content) as JsonValue
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

function saveDateTime() {
	const now = new Date()
	return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}_${now.getHours()}-${now.getMinutes()}-${now.getSeconds()}`
}

const jsonCodec = jsonCodecObject({
	updatedAt: jsonCodecString,
	checkpoint: jsonCodecRaw,
	dataStore: jsonCodecRaw,
})
