import {
  jsonTypeBoolean,
  jsonTypeInteger,
  jsonTypeNumber,
  jsonTypeString,
} from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp } from "solana-kiss-rpc";
import {
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
  index: jsonTypeInteger.decoder,
  authority: jsonTypeString.decoder,
  collateralMint: jsonTypeString.decoder,
  maxDepositCollateralAmount: jsonTypeInteger.decoder,
  totalDepositedCollateralAmount: jsonTypeInteger.decoder,
  totalExtractedCollateralAmount: jsonTypeInteger.decoder,
  claimingEnabled: jsonTypeBoolean.decoder,
  redeemableMint: jsonTypeString.decoder,
  totalClaimedRedeemableAmount: jsonTypeInteger.decoder,
  freeze: jsonTypeBoolean.decoder,
});
