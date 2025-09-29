import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import {
  jsonTypeBoolean,
  jsonTypeNumber,
  jsonTypeObjectWithKeyEncoder,
  jsonTypeString,
  jsonTypeStringToBigint,
  JsonValue,
} from "../json";
import { camelCaseToSnakeCase } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export async function miningPoolIndexingCheckpoint(
  dataStore: MiningPoolDataStore,
  idlService: ToolboxIdlService,
  endpoint: ToolboxEndpoint,
) {
  for (const [poolAddress, poolInfo] of dataStore.poolsInfos) {
    if (poolInfo.accountFetchedOrdering === poolInfo.accountRequestOrdering) {
      break;
    }
    try {
      const poolAccount = await idlService.getAndInferAndDecodeAccount(
        endpoint,
        new PublicKey(poolAddress),
      );
      const poolState = poolStateJsonType.decode(
        poolAccount.state as JsonValue,
      );
      dataStore.savePoolState(poolAddress, poolState);
    } catch (error) {
      console.error("Failed to refresh pool account state", poolAddress, error);
    }
  }
}

const poolStateJsonType = jsonTypeObjectWithKeyEncoder(
  {
    bump: jsonTypeNumber(),
    index: jsonTypeStringToBigint(),
    authority: jsonTypeString(),
    collateralMint: jsonTypeString(),
    maxDepositCollateralAmount: jsonTypeStringToBigint(),
    totalDepositedCollateralAmount: jsonTypeStringToBigint(),
    totalExtractedCollateralAmount: jsonTypeStringToBigint(),
    claimingEnabled: jsonTypeBoolean(),
    redeemableMint: jsonTypeString(),
    totalClaimedRedeemableAmount: jsonTypeStringToBigint(),
    freeze: jsonTypeBoolean(),
  },
  camelCaseToSnakeCase,
);
