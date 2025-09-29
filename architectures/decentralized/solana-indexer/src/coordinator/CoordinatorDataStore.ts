import {
  jsonTypeArrayToVariant,
  jsonTypeMapped,
  jsonTypeObject,
  jsonTypeObjectToMap,
} from "../json";
import {
  CoordinatorDataRunInfo,
  coordinatorDataRunInfoJsonType,
} from "./CoordinatorDataRunInfo";
import { CoordinatorDataRunState } from "./CoordinatorDataRunState";

export class CoordinatorDataStore {
  public runsInfos: Map<string, CoordinatorDataRunInfo>;

  constructor(runs: Map<string, CoordinatorDataRunInfo>) {
    this.runsInfos = runs;
  }

  public getRunInfo(runAddress: string): CoordinatorDataRunInfo {
    let runInfo = this.runsInfos.get(runAddress);
    if (runInfo === undefined) {
      runInfo = {
        accountState: undefined,
        accountFetchedOrdering: 0n,
        accountRequestOrdering: 0n,
      };
      this.runsInfos.set(runAddress, runInfo);
    }
    return runInfo;
  }

  public saveRunState(runAddress: string, runState: CoordinatorDataRunState) {
    let runInfo = this.getRunInfo(runAddress);
    runInfo.accountState = runState;
    runInfo.accountFetchedOrdering = runInfo.accountRequestOrdering;
  }

  public setRunRequestOrdering(runAddress: string, ordering: bigint) {
    const runInfo = this.getRunInfo(runAddress);
    if (ordering > runInfo.accountRequestOrdering) {
      runInfo.accountRequestOrdering = ordering;
    }
  }
}

const jsonTypeV1 = jsonTypeArrayToVariant(
  "Store(v1)",
  jsonTypeObject({
    runsInfos: jsonTypeObjectToMap(coordinatorDataRunInfoJsonType),
  }),
);

export const coordinatorDataStoreJsonType = jsonTypeMapped(jsonTypeV1, {
  map: (unmapped) => new CoordinatorDataStore(unmapped.runsInfos),
  unmap: (mapped) => ({ runsInfos: mapped.runsInfos }),
});
