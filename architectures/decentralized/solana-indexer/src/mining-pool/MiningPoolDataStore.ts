import {
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeRemap,
  JsonValue,
} from "solana-kiss-data";
import {
  MiningPoolDataPoolInfo,
  miningPoolDataPoolInfoJsonType,
} from "./MiningPoolDataPoolInfo";
import { MiningPoolDataPoolState } from "./MiningPoolDataPoolState";

export class MiningPoolDataStore {
  public poolsInfos: Map<string, MiningPoolDataPoolInfo>;

  constructor(pools: Map<string, MiningPoolDataPoolInfo>) {
    this.poolsInfos = pools;
  }

  public getPoolInfo(poolAddress: string): MiningPoolDataPoolInfo {
    let poolInfo = this.poolsInfos.get(poolAddress);
    if (poolInfo === undefined) {
      poolInfo = {
        accountState: undefined,
        accountFetchedOrdering: 0n,
        accountRequestOrdering: 0n,
        depositCollateralAmountPerUser: new Map<string, bigint>(),
        totalDepositCollateralAmount: 0n,
        claimRedeemableAmountPerUser: new Map<string, bigint>(),
        totalClaimRedeemableAmount: 0n,
        updates: [],
        claimables: [],
        totalExtractCollateralAmount: 0n,
      };
      this.poolsInfos.set(poolAddress, poolInfo);
    }
    return poolInfo;
  }

  public savePoolState(
    poolAddress: string,
    poolState: MiningPoolDataPoolState,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.accountState = poolState;
    poolInfo.accountFetchedOrdering = poolInfo.accountRequestOrdering;
  }

  public savePoolDeposit(
    poolAddress: string,
    userAddress: string,
    depositAmount: bigint,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    const depositAmountBefore =
      poolInfo.depositCollateralAmountPerUser.get(userAddress) ?? 0n;
    const depositAmountAfter = depositAmountBefore + depositAmount;
    poolInfo.depositCollateralAmountPerUser.set(
      userAddress,
      depositAmountAfter,
    );
    poolInfo.totalDepositCollateralAmount += depositAmount;
  }

  public savePoolClaim(
    poolAddress: string,
    userAddress: string,
    redeemableAmount: bigint,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    const redeemableAmountBefore =
      poolInfo.claimRedeemableAmountPerUser.get(userAddress) ?? 0n;
    const redeemableAmountAfter = redeemableAmountBefore + redeemableAmount;
    poolInfo.claimRedeemableAmountPerUser.set(
      userAddress,
      redeemableAmountAfter,
    );
    poolInfo.totalClaimRedeemableAmount += redeemableAmount;
  }

  public savePoolExtract(poolAddress: string, collateralAmount: bigint) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.totalExtractCollateralAmount += collateralAmount;
  }

  public savePoolUpdate(
    poolAddress: string,
    ordering: bigint,
    payload: JsonValue,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.updates.push({ ordering, payload });
    poolInfo.updates.sort((a, b) => Number(b.ordering - a.ordering));
  }

  public savePoolClaimable(
    poolAddress: string,
    ordering: bigint,
    payload: JsonValue,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    poolInfo.updates.push({ ordering, payload });
    poolInfo.updates.sort((a, b) => Number(b.ordering - a.ordering));
  }

  public setPoolRequestOrdering(poolAddress: string, ordering: bigint) {
    const poolInfo = this.getPoolInfo(poolAddress);
    if (ordering > poolInfo.accountRequestOrdering) {
      poolInfo.accountRequestOrdering = ordering;
    }
  }
}

export const miningPoolDataStoreJsonType = jsonTypeRemap(
  jsonTypeObject({
    poolsInfos: jsonTypeObjectToMap(miningPoolDataPoolInfoJsonType),
  }),
  (unmapped) => new MiningPoolDataStore(unmapped.poolsInfos),
  (remapped) => ({ poolsInfos: remapped.poolsInfos }),
);
