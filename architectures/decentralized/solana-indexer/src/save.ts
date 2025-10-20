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
	saveSubject: string,
	saveName: string,
	saveContent: {
		checkpoint: JsonValue
		dataStore: JsonValue
	}
): Promise<void> {
	const startTime = Date.now()
	const path = savePath(saveSubject, saveName, 'latest')
	const pathTemp = savePath(saveSubject, saveName, `tmp_${fileDateTime()}`)
	const pathBackup = savePath(saveSubject, saveName, `backup_${fileDateOnly()}`)
	const encoded = jsonCodec.encoder({
		updatedAt: new Date().toISOString(),
		checkpoint: saveContent.checkpoint,
		dataStore: saveContent.dataStore,
	})
	const content = JSON.stringify(encoded)
	await fsp.mkdir(dirname(pathBackup), { recursive: true })
	await fsp.writeFile(pathBackup, content, { flush: true })
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

export async function saveRead(
	saveSubject: string,
	saveName: string
): Promise<{
	updatedAt: string
	checkpoint: JsonValue
	dataStore: JsonValue
}> {
	const startTime = Date.now()
	const path = savePath(saveSubject, saveName, 'latest')
	const pathStarter = savePath(
		saveSubject,
		saveName,
		`started_${fileDateTime()}`
	)
	const content = await fsp.readFile(path, 'utf-8')
	await fsp.mkdir(dirname(pathStarter), { recursive: true })
	await fsp.writeFile(pathStarter, content, { flush: true })
	const encoded = JSON.parse(content) as JsonValue
	const decoded = jsonCodec.decoder(encoded)
	console.log(
		new Date().toISOString(),
		`Read ${saveSubject} ${saveName} in ${Date.now() - startTime}ms`
	)
	return decoded
}

function savePath(saveSubject: string, saveName: string, kind: string): string {
	return join(
		utilsGetStateDirectory(),
		'saves',
		saveSubject,
		`${saveName}.${kind}.json`
	)
}

function fileDateOnly() {
	const now = new Date()
	return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}`
}

function fileDateTime() {
	const now = new Date()
	return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}_${now.getHours()}-${now.getMinutes()}-${now.getSeconds()}`
}

const jsonCodec = jsonCodecObject({
	updatedAt: jsonCodecString,
	checkpoint: jsonCodecRaw,
	dataStore: jsonCodecRaw,
})
