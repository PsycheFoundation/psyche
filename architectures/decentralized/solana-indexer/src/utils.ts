import { mkdirSync, writeFileSync } from 'fs'
import { dirname, join } from 'path'
import {
	idlAccountDecode,
	idlOnchainAnchorAddress,
	idlOnchainAnchorDecode,
	IdlProgram,
	idlProgramGuessAccount,
	JsonCodec,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecObjectToMap,
	jsonCodecPubkey,
	JsonDecoder,
	jsonDecoderArray,
	jsonDecoderArrayToObject,
	jsonDecoderObject,
	jsonDecoderObjectWithKeysSnakeEncoded,
	jsonDecoderTransform,
	Pubkey,
	pubkeyFromBase58,
	pubkeyFromBytes,
	pubkeyToBase58,
	RpcHttp,
	rpcHttpGetAccountWithData,
} from 'solana-kiss'

export function utilsGetStateDirectory() {
	return process.env['STATE_DIRECTORY'] ?? process.cwd()
}
export function utilsGetEnv(name: string, description: string) {
	const value = process.env[name]
	if (!value) {
		throw new Error(`Missing ${description} in environment: ${name}`)
	}
	return value
}

export async function utilsGetProgramAnchorIdl(
	rpcHttp: RpcHttp,
	programAddress: Pubkey
): Promise<IdlProgram> {
	const onchainAnchorAddress = idlOnchainAnchorAddress(programAddress)
	const { accountInfo: onchainAnchorInfo } = await rpcHttpGetAccountWithData(
		rpcHttp,
		onchainAnchorAddress
	)
	if (onchainAnchorInfo.data.length === 0) {
		throw new Error('Idl account has no data')
	}
	return idlOnchainAnchorDecode(onchainAnchorInfo.data)
}
export async function utilsGetAndDecodeAccountState<Content>(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	accountAddress: Pubkey,
	accountDecoder: JsonDecoder<Content>
): Promise<Content> {
	const { accountInfo } = await rpcHttpGetAccountWithData(
		rpcHttp,
		accountAddress
	)
	if (accountInfo.data.length === 0) {
		throw new Error(`Failed to decode account with no data: ${accountAddress}`)
	}
	const accountIdl = idlProgramGuessAccount(programIdl, accountInfo.data)
	if (accountIdl === undefined) {
		throw new Error(`Failed to resolve Idl account type for: ${accountAddress}`)
	}
	return accountDecoder(idlAccountDecode(accountIdl, accountInfo.data))
}

export function utilsObjectToPubkeyMapJsonCodec<T>(
	valueType: JsonCodec<T>
): JsonCodec<Map<Pubkey, T>> {
	return jsonCodecObjectToMap(
		{
			keyDecoder: pubkeyFromBase58,
			keyEncoder: pubkeyToBase58,
		},
		valueType
	)
}
export function utilsObjectToStringMapJsonCodec<T>(
	valueType: JsonCodec<T>
): JsonCodec<Map<string, T>> {
	return jsonCodecObjectToMap(
		{
			keyDecoder: (key) => key,
			keyEncoder: (key) => key,
		},
		valueType
	)
}

export const utilsRustFixedStringJsonDecoder = jsonDecoderTransform(
	jsonDecoderArrayToObject({
		bytes: jsonDecoderArray(jsonCodecNumber.decoder),
	}),
	(encoded) => {
		let lastNonNull = 0
		for (let index = encoded.bytes.length - 1; index >= 0; index--) {
			if (encoded.bytes[index] !== 0) {
				lastNonNull = index + 1
				break
			}
		}
		return new TextDecoder().decode(
			new Uint8Array(encoded.bytes.slice(0, lastNonNull))
		)
	}
)
export function utilsRustFixedArrayJsonDecoder<T>(itemDecode: JsonDecoder<T>) {
	return jsonDecoderTransform(
		jsonDecoderObject({
			data: jsonDecoderArray(itemDecode),
			len: jsonCodecInteger.decoder,
		}),
		(encoded) => encoded.data.slice(0, Number(encoded.len))
	)
}
export const utilsRustSmallBooleanJsonDecoder = jsonDecoderTransform(
	jsonDecoderArrayToObject({ bit: jsonCodecNumber.decoder }),
	(encoded) => encoded.bit !== 0
)
export const utilsRustClientIdJsonDecoder =
	jsonDecoderObjectWithKeysSnakeEncoded({
		p2pIdentity: jsonDecoderTransform(
			jsonDecoderArray(jsonCodecNumber.decoder),
			(encoded) => pubkeyFromBytes(new Uint8Array(encoded))
		),
		signer: jsonCodecPubkey.decoder,
	})

export function utilsBigIntMax(a: bigint, b: bigint): bigint {
	return a > b ? a : b
}
export function utilsBigIntMin(a: bigint, b: bigint): bigint {
	return a < b ? a : b
}
export function utilsBigintArraySortAscending<Content>(
	array: Array<Content>,
	getKey: (item: Content) => bigint
) {
	array.sort((a, b) => {
		const aKey = getKey(a)
		const bKey = getKey(b)
		if (aKey < bKey) {
			return -1
		}
		if (aKey > bKey) {
			return 1
		}
		return 0
	})
}

export function utilsPlotPoints(
	directory: string,
	subject: string,
	category: string,
	points: {
		x: number | undefined
		y: number | undefined
	}[],
	xLabel?: (x: number) => string
) {
	const size = { x: 64, y: 14 }
	const pointsCleaned = points.filter(
		(p) =>
			p.x !== undefined &&
			p.y !== undefined &&
			Number.isFinite(p.x) &&
			Number.isFinite(p.y)
	) as Array<{ x: number; y: number }>
	const minX = Math.min(...pointsCleaned.map((p) => p.x))
	const maxX = Math.max(...pointsCleaned.map((p) => p.x))
	const minY = Math.min(...pointsCleaned.map((p) => p.y))
	const maxY = Math.max(...pointsCleaned.map((p) => p.y))
	function gridPos(point: { x: number; y: number }) {
		return {
			x: Math.round(((point.x - minX) / (maxX - minX)) * (size.x - 1)),
			y: Math.round(((point.y - minY) / (maxY - minY)) * (size.y - 1)),
		}
	}
	const grid = Array.from({ length: size.y }, () => Array(size.x).fill(0))
	for (const pointCleaned of pointsCleaned) {
		const pos = gridPos(pointCleaned)
		grid[pos.y]![pos.x]! += 1
	}
	const peak = Math.max(...grid.flat())
	const title = `${subject} - ${category}`
	const metaLeft = `@ ${new Date().toISOString()}`
	const metaRight = `${points.length.toString()} X`
	const intensities = [
		[' ', '.', ':', '-', '=', '+', '*', 'x', 'X', '#', '%', '@'],
		[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'],
		[' ', '░', '▒', '▓', '█'],
	][Math.round(Math.random() * 2)]!
	const lines: Array<string> = []
	lines.push(
		`${metaLeft.padEnd(size.x - metaRight.length + 2, ' ')}${metaRight}`
	)
	lines.push(`+${'-'.repeat(size.x)}+`)
	lines.push(
		`|${title.padStart(size.x / 2 + title.length / 2, ' ').padEnd(size.x)}|`
	)
	lines.push(`+${'-'.repeat(size.x)}+ --`)
	for (let rowIndex = grid.length - 1; rowIndex >= 0; rowIndex--) {
		const pixels = []
		for (let colIndex = 0; colIndex < grid[rowIndex]!.length; colIndex++) {
			const value = grid[rowIndex]![colIndex]!
			const pixel = Math.round((value / peak) * (intensities.length - 1))
			pixels.push(intensities[pixel])
		}
		const data = `|${pixels.join('')}|`
		if (rowIndex === grid.length - 1) {
			lines.push(`${data} ${maxY.toPrecision(6)}`)
		} else if (rowIndex === 0) {
			lines.push(`${data} ${minY.toPrecision(6)}`)
		} else {
			lines.push(data)
		}
	}
	lines.push(`+${'-'.repeat(size.x)}+ --`)
	const hx = size.x / 2 - 1
	const labelMinX = xLabel ? xLabel(minX) : minX.toString()
	const labelMaxX = xLabel ? xLabel(maxX) : maxX.toString()
	lines.push(`| ${labelMinX.padEnd(hx, ' ')}${labelMaxX.padStart(hx, ' ')} |`)
	const plotContent = lines.join('\n') + '\n'
	const plotPath = join(
		utilsGetStateDirectory(),
		`plots`,
		directory,
		subject,
		`${title}.txt`
	)
	mkdirSync(dirname(plotPath), { recursive: true })
	writeFileSync(plotPath, plotContent)
}
