export class MiningPoolDataStore {
  private depositPerUserPerPool: Map<string, Map<string, bigint>>;

  constructor(depositPerUserPerPool: Map<string, Map<string, bigint>>) {
    this.depositPerUserPerPool = depositPerUserPerPool;
  }

  public savePoolUserDeposit(pool: string, user: string, amount: bigint): void {
    let depositPerUser = this.depositPerUserPerPool.get(pool);
    if (!depositPerUser) {
      depositPerUser = new Map<string, bigint>();
      this.depositPerUserPerPool.set(pool, depositPerUser);
    }
    const beforeDeposit = depositPerUser.get(user) ?? 0n;
    const afterDeposit = beforeDeposit + amount;
    depositPerUser.set(user, afterDeposit);
  }

  public toJson(): any {
    const depositPerUserPerPoolObj: any = {};
    for (const [pool, depositPerUser] of this.depositPerUserPerPool.entries()) {
      const depositPerUserObj: any = {};
      for (const [user, amount] of depositPerUser.entries()) {
        depositPerUserObj[user] = amount.toString();
      }
      depositPerUserPerPoolObj[pool] = depositPerUserObj;
    }
    return {
      depositPerUserPerPool: depositPerUserPerPoolObj,
    };
  }

  public static fromJson(obj: any): MiningPoolDataStore {
    const depositPerUserPerPoolObj = obj?.["depositPerUserPerPool"];
    if (
      typeof depositPerUserPerPoolObj !== "object" ||
      depositPerUserPerPoolObj === null
    ) {
      throw new Error("Invalid depositPerUserPerPool");
    }
    const depositPerUserPerPool = new Map<string, Map<string, bigint>>();
    for (const [pool, depositPerUserObj] of Object.entries(
      depositPerUserPerPoolObj,
    )) {
      if (typeof depositPerUserObj !== "object" || depositPerUserObj === null) {
        throw new Error("Invalid depositPerUser");
      }
      const depositPerUser = new Map<string, bigint>();
      for (const [user, amountStr] of Object.entries(depositPerUserObj)) {
        if (typeof amountStr !== "string") {
          throw new Error("Invalid amount");
        }
        const amount = BigInt(amountStr);
        depositPerUser.set(user, amount);
      }
      depositPerUserPerPool.set(pool, depositPerUser);
    }
    return new MiningPoolDataStore(depositPerUserPerPool);
  }
}
