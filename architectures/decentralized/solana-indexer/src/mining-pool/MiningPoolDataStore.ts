import {
  jsonTypeArrayToVariant,
  jsonTypeMapped,
  jsonTypeObject,
  jsonTypeObjectToMap,
} from "../json";
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
        depositAmountPerUser: new Map<string, bigint>(),
        depositAmountTotal: 0n,
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

  public savePoolUserDeposit(
    poolAddress: string,
    userAddress: string,
    depositAmount: bigint,
  ) {
    let poolInfo = this.getPoolInfo(poolAddress);
    const depositAmountBefore =
      poolInfo.depositAmountPerUser.get(userAddress) ?? 0n;
    const depositAmountAfter = depositAmountBefore + depositAmount;
    poolInfo.depositAmountPerUser.set(userAddress, depositAmountAfter);
    poolInfo.depositAmountTotal += depositAmount;
  }

  public setPoolRequestOrdering(poolAddress: string, ordering: bigint) {
    const poolInfo = this.getPoolInfo(poolAddress);
    if (ordering > poolInfo.accountRequestOrdering) {
      poolInfo.accountRequestOrdering = ordering;
    }
  }
}

const jsonTypeV1 = jsonTypeArrayToVariant(
  "Store(v1)",
  jsonTypeObject({
    poolsInfos: jsonTypeObjectToMap(miningPoolDataPoolInfoJsonType),
  }),
);

export const miningPoolDataStoreJsonType = jsonTypeMapped(jsonTypeV1, {
  map: (unmapped) => new MiningPoolDataStore(unmapped.poolsInfos),
  unmap: (mapped) => ({ poolsInfos: mapped.poolsInfos }),
});
