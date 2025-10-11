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
	JsonDecoder,
	jsonDecoderArray,
	jsonDecoderArrayToObject,
	jsonDecoderObject,
	jsonDecoderTransform,
	Pubkey,
	pubkeyFromBase58,
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
