export interface MiningPoolDataStorePool {
  latestAccountState: MiningPoolDataStorePoolAccount | undefined;
  latestAccountOrdering: bigint;
  depositAmountPerUser: Map<string, bigint>;
}

export interface MiningPoolDataStorePoolAccount {}

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
      };
      this.pools.set(poolAddress, pool);
      return;
    }
    const depositAmountBefore =
      pool.depositAmountPerUser.get(userAddress) ?? 0n;
    const depositAmountAfter = depositAmountBefore + depositAmount;
    pool.depositAmountPerUser.set(userAddress, depositAmountAfter);
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
      };
      this.pools.set(poolAddress, pool);
    }
  }

  public getPools(): ReadonlyMap<string, Readonly<MiningPoolDataStorePool>> {
    return this.pools;
  }
}
