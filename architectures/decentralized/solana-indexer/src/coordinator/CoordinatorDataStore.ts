export class CoordinatorDataStore {
  constructor() {}

  public toJson(): any {
    return {};
  }

  public static fromJson(obj: any): CoordinatorDataStore {
    return new CoordinatorDataStore();
  }
}
