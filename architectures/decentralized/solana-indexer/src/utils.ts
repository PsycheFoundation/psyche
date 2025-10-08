import {
  idlAccountDecode,
  IdlProgram,
  idlProgramGuessAccount,
  idlStoreAnchorFind,
  idlStoreAnchorParse,
  JsonDecoder,
  jsonDecoderArray,
  jsonDecoderArrayToObject,
  jsonDecoderObject,
  jsonDecoderRemap,
  JsonType,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeObjectToMap,
  Pubkey,
  pubkeyFromBase58,
  pubkeyToBase58,
  RpcHttp,
  rpcHttpGetAccountWithData,
} from "solana-kiss";

export async function utilsGetProgramAnchorIdl(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
): Promise<IdlProgram> {
  const programIdlAddress = idlStoreAnchorFind(programAddress);
  const programIdlRecord = await rpcHttpGetAccountWithData(
    rpcHttp,
    programIdlAddress,
  );
  if (programIdlRecord.data.length === 0) {
    throw new Error("Idl account has no data");
  }
  return idlStoreAnchorParse(programIdlRecord.data);
}
export async function utilsGetAndDecodeAccountState<Content>(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  accountAddress: Pubkey,
  accountDecoder: JsonDecoder<Content>,
): Promise<Content> {
  const accountInfo = await rpcHttpGetAccountWithData(rpcHttp, accountAddress);
  if (accountInfo.data.length === 0) {
    throw new Error(`Failed to decode account with no data: ${accountAddress}`);
  }
  const accountIdl = idlProgramGuessAccount(programIdl, accountInfo.data);
  if (accountIdl === undefined) {
    throw new Error(
      `Failed to resolve Idl account type for: ${accountAddress}`,
    );
  }
  return accountDecoder(idlAccountDecode(accountIdl, accountInfo.data));
}

export function utilsObjectToPubkeyMapJsonType<T>(
  valueType: JsonType<T>,
): JsonType<Map<Pubkey, T>> {
  return jsonTypeObjectToMap(
    {
      keyDecoder: pubkeyFromBase58,
      keyEncoder: pubkeyToBase58,
    },
    valueType,
  );
}
export function utilsObjectToStringMapJsonType<T>(
  valueType: JsonType<T>,
): JsonType<Map<string, T>> {
  return jsonTypeObjectToMap(
    {
      keyDecoder: (key) => key,
      keyEncoder: (key) => key,
    },
    valueType,
  );
}

export const utilsRustFixedStringJsonDecoder = jsonDecoderRemap(
  jsonDecoderArrayToObject({
    bytes: jsonDecoderArray(jsonTypeNumber.decoder),
  }),
  (unmapped) => {
    let lastNonNull = 0;
    for (let index = unmapped.bytes.length - 1; index >= 0; index--) {
      if (unmapped.bytes[index] !== 0) {
        lastNonNull = index + 1;
        break;
      }
    }
    return new TextDecoder().decode(
      new Uint8Array(unmapped.bytes.slice(0, lastNonNull)),
    );
  },
);
export function utilsRustFixedArrayJsonDecoder<T>(itemDecode: JsonDecoder<T>) {
  return jsonDecoderRemap(
    jsonDecoderObject((key) => key, {
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
