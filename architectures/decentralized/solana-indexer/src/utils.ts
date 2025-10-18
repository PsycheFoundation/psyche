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
	title: string,
	size: { x: number; y: number },
	points: { x: number; y: number }[]
) {
	const pointsClean = points.filter(
		(p) => Number.isFinite(p.x) && Number.isFinite(p.y)
	)
	const minX = Math.min(...pointsClean.map((p) => p.x))
	const minY = Math.min(...pointsClean.map((p) => p.y))
	const maxX = Math.max(...pointsClean.map((p) => p.x))
	const maxY = Math.max(...pointsClean.map((p) => p.y))
	function gridPos(point: { x: number; y: number }) {
		return {
			x: Math.round(((point.x - minX) / (maxX - minX)) * (size.x - 1)),
			y: Math.round(((point.y - minY) / (maxY - minY)) * (size.y - 1)),
		}
	}
	const grid: string[][] = Array.from({ length: size.y }, () =>
		Array(size.x).fill(' ')
	)
	for (const point of pointsClean) {
		const pos = gridPos(point)
		grid[pos.y]![pos.x] = '*'
	}
	const hx = size.x / 2
	console.log('')
	console.log(`> ${title}`)
	console.log('+' + '-'.repeat(size.x) + '+' + ` ${maxY}`)
	for (let rowIndex = grid.length - 1; rowIndex >= 0; rowIndex--) {
		console.log('|' + grid[rowIndex]!.join('') + '|')
	}
	console.log('+' + '-'.repeat(size.x) + '+' + ` ${minY}`)
	console.log(
		`${minX.toString().padEnd(hx + 1, ' ')}`,
		`${maxX.toString().padStart(hx, ' ')}`
	)
	console.log('')
}
