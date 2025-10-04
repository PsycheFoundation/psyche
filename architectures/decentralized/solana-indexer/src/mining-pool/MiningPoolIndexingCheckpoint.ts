import {
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeString,
} from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import {
  utilsBigintStringJsonType,
  utilsGetAndDecodeAccountState,
  utilsObjectSnakeCaseJsonDecoder,
} from "../utils";
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
      const poolState = poolStateJsonDecoder(
        await utilsGetAndDecodeAccountState(rpcHttp, programIdl, poolAddress),
      );
      dataStore.savePoolState(poolAddress, poolState);
    } catch (error) {
      console.error("Failed to refresh pool account state", poolAddress, error);
    }
  }
}

const poolStateJsonDecoder = utilsObjectSnakeCaseJsonDecoder({
  bump: jsonTypeNumber.decoder,
  index: utilsBigintStringJsonType.decoder,
  authority: jsonTypeString.decoder,
  collateralMint: jsonTypeString.decoder,
  maxDepositCollateralAmount: utilsBigintStringJsonType.decoder,
  totalDepositedCollateralAmount: utilsBigintStringJsonType.decoder,
  totalExtractedCollateralAmount: utilsBigintStringJsonType.decoder,
  claimingEnabled: jsonTypeBoolean.decoder,
  redeemableMint: jsonTypeString.decoder,
  totalClaimedRedeemableAmount: utilsBigintStringJsonType.decoder,
  freeze: jsonTypeBoolean.decoder,
});
