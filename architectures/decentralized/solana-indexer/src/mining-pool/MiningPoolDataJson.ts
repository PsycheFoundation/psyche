import {
  JsonSchemaInfered,
  jsonSchemaNull,
  jsonSchemaNumberConst,
  jsonSchemaObject,
  jsonSchemaRecord,
  jsonSchemaString,
  jsonSchemaUnion,
  JsonValue,
} from "../json";
import {
  MiningPoolDataStore,
  MiningPoolDataStorePool,
} from "./MiningPoolDataStore";

const jsonSchemaV2 = jsonSchemaObject({
  version: jsonSchemaNumberConst(2),
  pools: jsonSchemaRecord(
    jsonSchemaObject({
      latestAccountState: jsonSchemaUnion(jsonSchemaNull(), jsonSchemaString()),
      latestAccountOrdering: jsonSchemaString(),
      depositAmountPerUser: jsonSchemaRecord(jsonSchemaString()),
    }),
  ),
});

const jsonSchemaV3 = jsonSchemaObject({
  version: jsonSchemaNumberConst(3),
  pools: jsonSchemaRecord(
    jsonSchemaObject({
      latestAccountState: jsonSchemaUnion(jsonSchemaNull(), jsonSchemaString()),
      latestAccountOrdering: jsonSchemaString(),
      depositAmountPerUser: jsonSchemaRecord(jsonSchemaString()),
    }),
  ),
});

export function miningPoolDataToJson(
  dataStore: MiningPoolDataStore,
): JsonValue {
  const pools: JsonSchemaInfered<typeof jsonSchemaV2>["pools"] = {};
  for (const [poolAddress, pool] of dataStore.getPools().entries()) {
    const depositAmountPerUser: Record<string, string> = {};
    for (const [user, amount] of pool.depositAmountPerUser.entries()) {
      depositAmountPerUser[user] = amount.toString();
    }
    pools[poolAddress] = {
      // TODO - implement properly
      latestAccountState:
        pool.latestAccountState === undefined
          ? null
          : String(pool.latestAccountState),
      latestAccountOrdering: String(pool.latestAccountOrdering),
      depositAmountPerUser,
    };
  }
  return jsonSchemaV2.guard({
    version: 2,
    pools,
  });
}

export function miningPoolDataFromJson(
  jsonValue: JsonValue,
): MiningPoolDataStore {
  const jsonParsedV2 = jsonSchemaV2.parse(jsonValue);
  const pools = new Map<string, MiningPoolDataStorePool>();
  for (const [poolAddress, poolParsed] of Object.entries(jsonParsed.pools)) {
    const depositAmountPerUser = new Map<string, bigint>();
    for (const [userAddress, amountValue] of Object.entries(
      poolParsed.depositAmountPerUser,
    )) {
      depositAmountPerUser.set(userAddress, BigInt(amountValue));
    }
    pools.set(poolAddress, {
      // TODO - implement properly
      latestAccountState:
        poolParsed.latestAccountState === null
          ? undefined
          : poolParsed.latestAccountState,
      latestAccountOrdering: BigInt(poolParsed.latestAccountOrdering),
      depositAmountPerUser,
    });
  }
  return new MiningPoolDataStore(pools);
}
