import {
	JsonCodec,
	JsonDecoder,
	Pubkey,
	jsonCodecBigInt,
	jsonCodecNumber,
	jsonCodecObjectToMap,
	jsonCodecPubkey,
	jsonDecoderArrayToArray,
	jsonDecoderArrayToObject,
	jsonDecoderObjectToObject,
	jsonDecoderWrapped,
	pubkeyFromBase58,
	pubkeyFromBytes,
	pubkeyToBase58,
} from 'solana-kiss'

export function jsonCodecObjectToMapByPubkey<T>(
	valueCodec: JsonCodec<T>
): JsonCodec<Map<Pubkey, T>> {
	return jsonCodecObjectToMap({
		keyCodec: {
			decoder: pubkeyFromBase58,
			encoder: pubkeyToBase58,
		},
		valueCodec,
	})
}

export const jsonDecoderRustFixedString = jsonDecoderWrapped(
	jsonDecoderArrayToObject({
		bytes: jsonDecoderArrayToArray(jsonCodecNumber.decoder),
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

export function jsonDecoderRustFixedArray<T>(itemDecode: JsonDecoder<T>) {
	return jsonDecoderWrapped(
		jsonDecoderObjectToObject({
			data: jsonDecoderArrayToArray(itemDecode),
			len: jsonCodecBigInt.decoder,
		}),
		(encoded) => encoded.data.slice(0, Number(encoded.len))
	)
}

export const jsonDecoderRustSmallBoolean = jsonDecoderWrapped(
	jsonDecoderArrayToObject({ bit: jsonCodecNumber.decoder }),
	(encoded) => encoded.bit !== 0
)

export const jsonDecoderRustClientId = jsonDecoderObjectToObject({
	p2pIdentity: jsonDecoderWrapped(
		jsonDecoderArrayToArray(jsonCodecNumber.decoder),
		(encoded) => pubkeyFromBytes(new Uint8Array(encoded))
	),
	signer: jsonCodecPubkey.decoder,
})
