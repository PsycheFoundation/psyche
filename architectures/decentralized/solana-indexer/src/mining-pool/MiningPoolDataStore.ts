import { JsonValue, Pubkey, jsonTypeObject, jsonTypeRemap } from "solana-kiss";
import { utilsObjectToPubkeyMapJsonType } from "../utils";
import {
  MiningPoolDataPoolInfo,
  miningPoolDataPoolInfoJsonType,
} from "./MiningPoolDataPoolInfo";
import { MiningPoolDataPoolState } from "./MiningPoolDataPoolState";

export class MiningPoolDataStore {
  public poolsInfos: Map<Pubkey, MiningPoolDataPoolInfo>;

  constructor(pools: Map<Pubkey, MiningPoolDataPoolInfo>) {
    this.poolsInfos = pools;
  }

  public getPoolInfo(poolAddress: Pubkey): MiningPoolDataPoolInfo {
    let poolInfo = this.poolsInfos.get(poolAddress);
    if (poolInfo === undefined) {
      poolInfo = {
        accountState: undefined,
        accountUpdatedAt: undefined,
        accountFetchedOrdering: 0n,
        accountRequestOrdering: 0n,
        totalExtractCollateralAmount: 0n,
        depositCollateralAmountPerUser: new Map<Pubkey, bigint>(),
        totalDepositCollateralAmount: 0n,
        claimRedeemableAmountPerUser: new Map<Pubkey, bigint>(),
        totalClaimRedeemableAmount: 0n,
        adminHistory: [],
      };
      this.poolsInfos.set(poolAddress, poolInfo);
    }
    return poolInfo;
  }

  public savePoolState(
    poolAddress: Pubkey,
    poolState: MiningPoolDataPoolState,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.accountState = poolState;
    poolInfo.accountUpdatedAt = new Date();
    poolInfo.accountFetchedOrdering = poolInfo.accountRequestOrdering;
  }

  public savePoolExtract(poolAddress: Pubkey, collateralAmount: bigint) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.totalExtractCollateralAmount += collateralAmount;
  }

  public savePoolDeposit(
    poolAddress: Pubkey,
    signerAddress: Pubkey,
    depositAmount: bigint,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    const depositAmountBefore =
      poolInfo.depositCollateralAmountPerUser.get(signerAddress) ?? 0n;
    const depositAmountAfter = depositAmountBefore + depositAmount;
    poolInfo.depositCollateralAmountPerUser.set(
      signerAddress,
      depositAmountAfter,
    );
    poolInfo.totalDepositCollateralAmount += depositAmount;
  }

  public savePoolClaim(
    poolAddress: Pubkey,
    signerAddress: Pubkey,
    redeemableAmount: bigint,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    const redeemableAmountBefore =
      poolInfo.claimRedeemableAmountPerUser.get(signerAddress) ?? 0n;
    const redeemableAmountAfter = redeemableAmountBefore + redeemableAmount;
    poolInfo.claimRedeemableAmountPerUser.set(
      signerAddress,
      redeemableAmountAfter,
    );
    poolInfo.totalClaimRedeemableAmount += redeemableAmount;
  }

  public savePoolAdminAction(
    poolAddress: Pubkey,
    signerAddress: Pubkey,
    instructionName: string,
    instructionAddresses: Map<string, Pubkey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    processedTime: Date | undefined,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.adminHistory.push({
      processedTime,
      ordering,
      signerAddress,
      instructionName,
      instructionAddresses,
      instructionPayload,
    });
    poolInfo.adminHistory.sort((a, b) => Number(b.ordering - a.ordering));
  }

  public setPoolRequestOrdering(poolAddress: Pubkey, ordering: bigint) {
    const poolInfo = this.getPoolInfo(poolAddress);
    if (ordering > poolInfo.accountRequestOrdering) {
      poolInfo.accountRequestOrdering = ordering;
    }
  }
}

export const miningPoolDataStoreJsonType = jsonTypeRemap(
  jsonTypeObject((key) => key, {
    poolsInfos: utilsObjectToPubkeyMapJsonType(miningPoolDataPoolInfoJsonType),
  }),
  (unmapped) => new MiningPoolDataStore(unmapped.poolsInfos),
  (remapped) => ({ poolsInfos: remapped.poolsInfos }),
);
