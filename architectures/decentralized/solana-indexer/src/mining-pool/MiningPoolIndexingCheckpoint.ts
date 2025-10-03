import {
  camelCaseToSnakeCase,
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeString,
} from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import { getAndDecodeAccountState, jsonTypeStringToBigint } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingCheckpoint(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  dataStore: MiningPoolDataStore,
) {
  for (const [poolAddress, poolInfo] of dataStore.poolsInfos) {
    if (poolInfo.accountFetchedOrdering === poolInfo.accountRequestOrdering) {
      break;
    }
    try {
      const poolState = poolInfoJsonType.decode(
        await getAndDecodeAccountState(rpcHttp, programIdl, poolAddress),
      );
      dataStore.savePoolState(poolAddress, poolState);
    } catch (error) {
      console.error("Failed to refresh pool account state", poolAddress, error);
    }
  }
}

const poolStateJsonType = jsonTypeObjectWithKeyEncoder(
  {
    bump: jsonTypeNumber,
    index: jsonTypeStringToBigint,
    authority: jsonTypeString,
    collateralMint: jsonTypeString,
    maxDepositCollateralAmount: jsonTypeStringToBigint,
    totalDepositedCollateralAmount: jsonTypeStringToBigint,
    totalExtractedCollateralAmount: jsonTypeStringToBigint,
    claimingEnabled: jsonTypeBoolean,
    redeemableMint: jsonTypeString,
    totalClaimedRedeemableAmount: jsonTypeStringToBigint,
    freeze: jsonTypeBoolean,
  },
  camelCaseToSnakeCase,
);
