import {
  IdlProgram,
  jsonCodecBoolean,
  jsonCodecInteger,
  jsonCodecNumber,
  jsonCodecString,
  jsonDecoderObjectWithKeysSnakeEncoded,
  RpcHttp,
} from "solana-kiss";
import { utilsGetAndDecodeAccountState } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingCheckpoint(
  rpcHttp: RpcHttp,
  programIdl: IdlProgram,
  dataStore: MiningPoolDataStore,
) {
  for (const [poolAddress, poolInfo] of dataStore.poolInfoByAddress) {
    if (
      poolInfo.changeAcknowledgedOrdinal === poolInfo.changeNotificationOrdinal
    ) {
      continue;
    }
    poolInfo.changeAcknowledgedOrdinal = poolInfo.changeNotificationOrdinal;
    try {
      const poolState = await utilsGetAndDecodeAccountState(
        rpcHttp,
        programIdl,
        poolAddress,
        poolStateJsonDecoder,
      );
      let poolInfo = dataStore.getPoolInfo(poolAddress);
      poolInfo.accountUpdatedAt = new Date();
      poolInfo.accountState = poolState;
    } catch (error) {
      console.error("Failed to refresh pool account state", poolAddress, error);
    }
  }
}

const poolStateJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
  bump: jsonCodecNumber.decoder,
  index: jsonCodecInteger.decoder,
  authority: jsonCodecString.decoder,
  collateralMint: jsonCodecString.decoder,
  maxDepositCollateralAmount: jsonCodecInteger.decoder,
  totalDepositedCollateralAmount: jsonCodecInteger.decoder,
  totalExtractedCollateralAmount: jsonCodecInteger.decoder,
  claimingEnabled: jsonCodecBoolean.decoder,
  redeemableMint: jsonCodecString.decoder,
  totalClaimedRedeemableAmount: jsonCodecInteger.decoder,
  freeze: jsonCodecBoolean.decoder,
});
