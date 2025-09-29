import {
  JsonType,
  jsonTypeMapped,
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeObjectToVariant,
  jsonTypeWithDecodeFallbacks,
} from "../json";
import { Immutable } from "../utils";
import {
  MiningPoolDataPoolAccountState,
  MiningPoolDataPoolDetails,
  miningPoolDataPoolDetailsJsonType,
} from "./MiningPoolDataPoolDetails";

export class MiningPoolDataStore {
  private pools: Map<string, MiningPoolDataPoolDetails>;

  constructor(pools: Map<string, MiningPoolDataPoolDetails>) {
    this.pools = pools;
  }

  public getPools(): Immutable<Map<string, MiningPoolDataPoolDetails>> {
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
    poolAccountState: MiningPoolDataPoolAccountState,
  ) {
    console.log("Saving pool account state", poolAddress, poolAccountState);
    let pool = this.pools.get(poolAddress);
    if (pool != undefined) {
      pool.latestAccountState = poolAccountState;
    } else {
      pool = {
        latestAccountState: poolAccountState,
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

const jsonTypeV1 = jsonTypeObjectToVariant(
  "mining_pool_data_v1",
  jsonTypeObject({
    pools: jsonTypeObjectToMap(miningPoolDataPoolDetailsJsonType),
  }),
);

export const miningPoolDataStoreJsonType: JsonType<MiningPoolDataStore> =
  jsonTypeMapped(jsonTypeWithDecodeFallbacks(jsonTypeV1, []), {
    map: (unmapped) => new MiningPoolDataStore(unmapped.pools),
    unmap: (mapped) => ({
      pools: mapped.getPools(),
    }),
  });
