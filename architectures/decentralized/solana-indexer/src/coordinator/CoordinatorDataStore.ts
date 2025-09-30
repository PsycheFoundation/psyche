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
        witnessesPerUser: new Map(),
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

  public saveRunWitness(
    runAddress: string,
    userAddress: string,
    ordering: bigint,
    metadata: {
      tokensPerSec: number;
      bandwidthPerSec: number;
      loss: number;
      step: number;
    },
  ) {
    const runInfo = this.getRunInfo(runAddress);
    const userWitnesses = runInfo.witnessesPerUser.get(userAddress) ?? {
      lastFew: [],
      sampled: {
        rate: 1,
        data: [],
      },
    };
    const witness = {
      ordering,
      metadata,
    };

    const targetCount = 3;

    userWitnesses.lastFew.push(witness);
    userWitnesses.lastFew.sort((a, b) => Number(a.ordering - b.ordering));
    userWitnesses.lastFew = userWitnesses.lastFew.slice(-targetCount);

    if (Math.random() < 1 / userWitnesses.sampled.rate) {
      userWitnesses.sampled.data.push(witness);
      userWitnesses.sampled.data.sort((a, b) =>
        Number(a.ordering - b.ordering),
      );
      while (userWitnesses.sampled.data.length > targetCount) {
        userWitnesses.sampled.rate *= 2;
        userWitnesses.sampled.data = userWitnesses.sampled.data.filter(
          () => Math.random() < 0.5,
        );
      }
    }

    runInfo.witnessesPerUser.set(userAddress, userWitnesses);
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
