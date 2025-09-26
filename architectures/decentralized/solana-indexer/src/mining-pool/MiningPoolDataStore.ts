import { Immutable } from "../utils";

export interface MiningPoolDataStorePool {
  latestAccountState: MiningPoolDataStorePoolAccount | undefined;
  latestAccountOrdering: bigint;
  depositAmountPerUser: Map<string, bigint>;
  computedTotal1: bigint;
  computedTotal2: bigint;
}

export interface MiningPoolDataStorePoolAccount {
  bump: number;
  index: bigint;
  authority: string;
  collateral_mint: string;
  max_deposit_collateral_amount: bigint;
  total_deposited_collateral_amount: bigint;
  total_extracted_collateral_amount: bigint;
  claiming_enabled: boolean;
  redeemable_mint: string;
  total_claimed_redeemable_amount: bigint;
  freeze: boolean;
}

export class MiningPoolDataStore {
  private pools: Map<string, MiningPoolDataStorePool>;

  constructor(poolsInfos: Map<string, MiningPoolDataStorePool>) {
    this.pools = poolsInfos;
  }

  public invalidatePoolAccountState(
    poolAddress: string,
    ordering: bigint,
  ): void {
    const pool = this.pools.get(poolAddress);
    if (pool === undefined) {
      return;
    }
    if (ordering > pool.latestAccountOrdering) {
      pool.latestAccountState = undefined;
      pool.latestAccountOrdering = ordering;
    }
  }

  public getInvalidatedPoolsAddresses(): string[] {
    const dirtyPools: string[] = [];
    for (const [poolAddress, pool] of this.pools.entries()) {
      if (pool.latestAccountState === undefined) {
        dirtyPools.push(poolAddress);
      }
    }
    return dirtyPools;
  }

  public savePoolUserDeposit(
    ordering: bigint,
    poolAddress: string,
    userAddress: string,
    depositAmount: bigint,
  ): void {
    let pool = this.pools.get(poolAddress);
    if (pool === undefined) {
      pool = {
        latestAccountState: undefined,
        latestAccountOrdering: ordering,
        depositAmountPerUser: new Map<string, bigint>(),
        computedTotal1: 0n,
        computedTotal2: 0n,
      };
      this.pools.set(poolAddress, pool);
      return;
    }
    const depositAmountBefore =
      pool.depositAmountPerUser.get(userAddress) ?? 0n;
    const depositAmountAfter = depositAmountBefore + depositAmount;
    pool.depositAmountPerUser.set(userAddress, depositAmountAfter);

    pool.computedTotal1 = pool.computedTotal1 + depositAmount;

    let total2 = 0n;
    for (const depositAmount of pool.depositAmountPerUser.values()) {
      total2 += depositAmount;
    }
    pool.computedTotal2 = total2;
    this.invalidatePoolAccountState(poolAddress, ordering);
  }

  public savePoolAccountState(
    poolAddress: string,
    accountState: MiningPoolDataStorePoolAccount,
  ) {
    let pool = this.pools.get(poolAddress);
    if (pool != undefined) {
      pool.latestAccountState = accountState;
    } else {
      pool = {
        latestAccountState: accountState,
        latestAccountOrdering: 0n,
        depositAmountPerUser: new Map<string, bigint>(),
        computedTotal1: 0n,
        computedTotal2: 0n,
      };
      this.pools.set(poolAddress, pool);
    }
  }

  public getPools(): Immutable<Map<string, MiningPoolDataStorePool>> {
    return this.pools;
  }
}
