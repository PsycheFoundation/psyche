import {
  casingCamelToSnake,
  JsonDecoder,
  jsonDecoderArray,
  jsonDecoderArrayToObject,
  jsonDecoderObject,
  jsonDecoderRemap,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeRemap,
  jsonTypeString,
  Pubkey,
} from "solana-kiss-data";
import {
  idlAccountDecode,
  IdlProgram,
  idlProgramGuessAccount,
} from "solana-kiss-idl";
import { RpcHttp, rpcHttpGetAccountWithData } from "solana-kiss-rpc";

export async function utilsGetAndDecodeAccountState(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  accountAddress: Pubkey,
) {
  const accountRecord = await rpcHttpGetAccountWithData(
    rpcHttp,
    accountAddress,
  );
  const accountIdl = idlProgramGuessAccount(programIdl, accountRecord.data);
  if (accountIdl === undefined) {
    throw new Error(
      `Failed to resolve Idl account type for: ${accountAddress}`,
    );
  }
  return idlAccountDecode(accountIdl, accountRecord.data);
}

export function utilsObjectSnakeCaseJsonDecoder<
  Shape extends { [key: string]: JsonDecoder<any> },
>(shape: Shape) {
  const keysEncoding: { [K in keyof Shape]?: string } = {};
  for (const keyDecoded in shape) {
    keysEncoding[keyDecoded] = casingCamelToSnake(keyDecoded);
  }
  return jsonDecoderObject(shape, keysEncoding);
}

export const utilsRustFixedStringJsonDecoder = jsonDecoderRemap(
  jsonDecoderArrayToObject({
    bytes: jsonDecoderArray(jsonTypeNumber.decoder),
  }),
  (unmapped) => {
    const bytes = unmapped.bytes;
    const nulIndex = bytes.indexOf(0);
    const trimmed = nulIndex >= 0 ? bytes.slice(0, nulIndex) : bytes;
    return new TextDecoder().decode(new Uint8Array(trimmed));
  },
);

export function utilsRustFixedArrayJsonDecoder<T>(itemDecode: JsonDecoder<T>) {
  return jsonDecoderRemap(
    jsonDecoderObject({
      data: jsonDecoderArray(itemDecode),
      len: jsonTypeInteger.decoder,
    }),
    (unmapped) => unmapped.data.slice(0, Number(unmapped.len)),
  );
}

export const utilsRustSmallBooleanJsonDecoder = jsonDecoderRemap(
  jsonDecoderArrayToObject({ bit: jsonTypeNumber.decoder }),
  (unmapped) => unmapped.bit !== 0,
);

export const utilsOrderingJsonType = jsonTypeRemap(
  jsonTypeString,
  (encoded) => {
    return BigInt(encoded.split(":").join(""));
  },
  (decoded) => {
    return [
      decoded / 1_000_000_000n,
      (decoded / 1_000_000n) % 1_000n,
      (decoded / 1_000n) % 1_000n,
      decoded % 1_000n,
    ]
      .map((p) => p.toString().padStart(3, "0"))
      .join(":");
  },
);
