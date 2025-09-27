import { Immutable } from "../utils";

export interface MiningPoolDataStorePoolAccount {
  bump: number;
  index: bigint;
  authority: string;
  collateralMint: string;
  maxDepositCollateralAmount: bigint;
  totalDepositedCollateralAmount: bigint;
  totalExtractedCollateralAmount: bigint;
  claimingEnabled: boolean;
  redeemableMint: string;
  totalClaimedRedeemableAmount: bigint;
  freeze: boolean;
}

export interface MiningPoolDataStorePool {
  latestAccountState: MiningPoolDataStorePoolAccount | undefined;
  latestAccountOrdering: bigint;
  depositAmountPerUser: Map<string, bigint>;
}

export class MiningPoolDataStore {
  private pools: Map<string, MiningPoolDataStorePool>;

  constructor(pools: Map<string, MiningPoolDataStorePool>) {
    this.pools = pools;
  }

  public getPools(): Immutable<Map<string, MiningPoolDataStorePool>> {
    return this.pools;
  }

  public savePoolUserDeposit(
    poolAddress: string,
    userAddress: string,
    depositAmount: bigint,
    ordering: bigint,
  ) {
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
  }

  public savePoolAccountState(
    poolAddress: string,
    accountState: MiningPoolDataStorePoolAccount,
  ) {
    console.log("Saving pool account state", poolAddress, accountState);
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

  public invalidatePoolAccountState(poolAddress: string, ordering: bigint) {
    const pool = this.pools.get(poolAddress);
    if (pool === undefined) {
      return;
    }
    if (ordering > pool.latestAccountOrdering) {
      pool.latestAccountState = undefined;
      pool.latestAccountOrdering = ordering;
    }
  }

  public getInvalidatedPoolsAddresses(): Array<string> {
    const dirtyPools: Array<string> = [];
    for (const [poolAddress, pool] of this.pools.entries()) {
      if (pool.latestAccountState === undefined) {
        dirtyPools.push(poolAddress);
      }
    }
    return dirtyPools;
  }
}
