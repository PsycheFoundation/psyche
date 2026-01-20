import {
  JsonCodec,
  JsonDecoder,
  JsonEncoder,
  JsonObject,
  Pubkey,
  jsonCodecBigInt,
  jsonCodecNumber,
  jsonCodecObject,
  jsonCodecObjectToMap,
  jsonCodecPubkey,
  jsonDecoderArrayToArray,
  jsonDecoderArrayToObject,
  jsonDecoderObjectToObject,
  jsonDecoderWrapped,
  objectGetOwnProperty,
  pubkeyFromBase58,
  pubkeyFromBytes,
  pubkeyToBase58,
  withErrorContext,
} from "solana-kiss";

export function jsonCodecObjectToMapByPubkey<T>(
  valueCodec: JsonCodec<T>,
): JsonCodec<Map<Pubkey, T>> {
  return jsonCodecObjectToMap({
    keyCodec: { decoder: pubkeyFromBase58, encoder: pubkeyToBase58 },
    valueCodec,
  });
}

export const jsonDecoderRustFixedString = jsonDecoderWrapped(
  jsonDecoderArrayToObject({
    bytes: jsonDecoderArrayToArray(jsonCodecNumber.decoder),
  }),
  (encoded) => {
    let lastNonNull = 0;
    for (let index = encoded.bytes.length - 1; index >= 0; index--) {
      if (encoded.bytes[index] !== 0) {
        lastNonNull = index + 1;
        break;
      }
    }
    return new TextDecoder().decode(
      new Uint8Array(encoded.bytes.slice(0, lastNonNull)),
    );
  },
);

export function jsonDecoderRustFixedArray<T>(itemDecode: JsonDecoder<T>) {
  return jsonDecoderWrapped(
    jsonDecoderObjectToObject({
      data: jsonDecoderArrayToArray(itemDecode),
      len: jsonCodecBigInt.decoder,
    }),
    (encoded) => encoded.data.slice(0, Number(encoded.len)),
  );
}

export const jsonDecoderRustSmallBoolean = jsonDecoderWrapped(
  jsonDecoderArrayToObject({ bit: jsonCodecNumber.decoder }),
  (encoded) => encoded.bit !== 0,
);

export const jsonDecoderRustClientId = jsonDecoderObjectToObject({
  p2pIdentity: jsonDecoderWrapped(
    jsonDecoderArrayToArray(jsonCodecNumber.decoder),
    (encoded) => pubkeyFromBytes(new Uint8Array(encoded)),
  ),
  signer: jsonCodecPubkey.decoder,
});

export function jsonDecoderObjectToRecord<Value>(
  valueDecoder: JsonDecoder<Value>,
): JsonDecoder<Record<string, Value>> {
  return (encoded) => {
    const decoded = {} as Record<string, Value>;
    const object = jsonCodecObject.decoder(encoded);
    for (const key in object) {
      const valueEncoded = objectGetOwnProperty(object, key);
      if (valueEncoded === undefined) {
        continue;
      }
      const valueDecoded = withErrorContext(
        `JSON: Decode Object["${key}"] (${key}) =>`,
        () => valueDecoder(valueEncoded),
      );
      decoded[key] = valueDecoded;
    }
    return decoded;
  };
}
export function jsonEncoderObjectToRecord<Value>(
  valueEncoder: JsonEncoder<Value>,
): JsonEncoder<Record<string, Value>> {
  return (decoded) => {
    const encoded = {} as JsonObject;
    for (const key in decoded) {
      const valueDecoded = decoded[key]!;
      const valueEncoded = valueEncoder(valueDecoded);
      encoded[key] = valueEncoded;
    }
    return encoded;
  };
}
export function jsonCodecObjectToRecord<Value>(
  valueCodec: JsonCodec<Value>,
): JsonCodec<Record<string, Value>> {
  return {
    decoder: jsonDecoderObjectToRecord(valueCodec.decoder),
    encoder: jsonEncoderObjectToRecord(valueCodec.encoder),
  };
}
