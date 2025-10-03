import { JsonType, jsonTypeArray, jsonTypeArrayToTuple, jsonTypeMap, jsonTypeNumber, jsonTypeObject, jsonTypeString, Pubkey } from "solana-kiss-data";
import { idlAccountDecode, IdlProgram, idlProgramGuessAccount } from "solana-kiss-idl";
import { RpcHttp, rpcHttpGetAccountWithData } from "solana-kiss-rpc";

export async function getAndDecodeAccountState(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  accountAddress: Pubkey,
) {
  const accountRecord = await rpcHttpGetAccountWithData(rpcHttp, accountAddress);
  const accountIdl = idlProgramGuessAccount(programIdl, accountRecord.data);
  if (accountIdl === undefined) {
    throw new Error(`Failed to resolve Idl account type for: ${accountAddress}`);
  }
  const accountState = idlAccountDecode(accountIdl, accountRecord.data);
  return accountState;
}

export function jsonTypeObjectSnakeCase<T>(fields: {
  [key in keyof T]: JsonType<T[key]>;
}) {
  return jsonTypeObjectWithKeyEncoder(fields, camelCaseToSnakeCase);
}

export function jsonTypeRustFixedString() {
  return jsonTypeMap(
    jsonTypeArrayToTuple([jsonTypeArray(jsonTypeNumber)]),
   (unmapped) => {
        const bytes = unmapped[0];
        const nulIndex = bytes.indexOf(0);
        const trimmed = nulIndex >= 0 ? bytes.slice(0, nulIndex) : bytes;
        return {
          value: new TextDecoder().decode(new Uint8Array(trimmed)),
          length: bytes.length,
        };
      }, (mapped) => {
        const bytes = new TextEncoder().encode(mapped.value);
        const padded = new Uint8Array(mapped.length);
        padded.set(bytes);
        return [Array.from(padded)] as [number[]];
      },
    
  );
}

export function jsonTypeRustFixedArray<T>(itemType: JsonType<T>) {
  return jsonTypeMap(
    jsonTypeObject({
      data: jsonTypeArray(itemType),
      len: jsonTypeStringToBigint(),
    }),
    
      (unmapped) => unmapped.data.slice(0, Number(unmapped.len)),
      (mapped) => ({ data: mapped, len: BigInt(mapped.length) }),
    
  );
}

export function jsonTypeRustSmallBoolean() {
  return jsonTypeMap(jsonTypeArrayToTuple([jsonTypeNumber]), {
    map: (unmapped) => unmapped[0] !== 0,
    unmap: (mapped) => (mapped ? ([1] as const) : ([0] as const)),
  });
}

export function jsonTypeStringToBigint() {
  return jsonTypeMap(jsonTypeString, 
    map: (unmapped) => BigInt(unmapped),
    unmap: (mapped) => mapped.toString(),
  );
}
