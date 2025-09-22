import {
  MiningPoolDataStore,
  MiningPoolDataStorePool,
} from "./MiningPoolDataStore";

export function miningPoolDataToJson(dataStore: MiningPoolDataStore): any {
  const pools: any = {};
  for (const [poolKey, poolValue] of dataStore.getPools().entries()) {
    const latestAccountState = poolValue.latestAccountState; // TODO - implement properly
    const latestAccountOrdering = poolValue.latestAccountOrdering.toString();
    const depositAmountPerUser: any = {};
    for (const [user, amount] of poolValue.depositAmountPerUser.entries()) {
      depositAmountPerUser[user] = amount.toString();
    }
    pools[poolKey] = {
      latestAccountState,
      latestAccountOrdering,
      depositAmountPerUser,
    };
  }
  return {
    version: 2,
    pools,
  };
}

export function miningPoolDataFromJson(json: any): MiningPoolDataStore {
  if (json?.["version"] !== 2) {
    throw new Error("Unsupported version");
  }
  // TODO - de-uglify this
  const poolsInfos = new Map<string, MiningPoolDataStorePool>();
  const poolsJson = json?.["pools"];
  if (poolsJson && typeof poolsJson === "object") {
    for (const [poolKey, poolValueRaw] of Object.entries(poolsJson)) {
      const poolValue: any = poolValueRaw;
      if (typeof poolKey !== "string" || typeof poolValue !== "object") {
        continue;
      }
      const latestAccountState = poolValue?.["latestAccountState"]; // TODO - implement properly
      const latestAccountOrderingStr = poolValue?.["latestAccountOrdering"];
      if (typeof latestAccountOrderingStr !== "string") {
        continue;
      }
      let latestAccountOrdering: bigint;
      try {
        latestAccountOrdering = BigInt(latestAccountOrderingStr);
      } catch (error) {
        continue;
      }
      const depositAmountPerUser = new Map<string, bigint>();
      const depositAmountPerUserJson = poolValue?.["depositAmountPerUser"];
      if (
        depositAmountPerUserJson &&
        typeof depositAmountPerUserJson === "object"
      ) {
        for (const [user, amountStr] of Object.entries(
          depositAmountPerUserJson,
        )) {
          if (typeof user !== "string" || typeof amountStr !== "string") {
            continue;
          }
          let amount: bigint;
          try {
            amount = BigInt(amountStr);
          } catch (error) {
            continue;
          }
          depositAmountPerUser.set(user, amount);
        }
      }
      poolsInfos.set(poolKey, {
        latestAccountState,
        latestAccountOrdering,
        depositAmountPerUser,
      });
    }
  }
  return new MiningPoolDataStore(poolsInfos);
}
