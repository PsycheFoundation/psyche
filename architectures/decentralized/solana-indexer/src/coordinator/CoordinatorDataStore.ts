import {
  jsonTypeObject,
  jsonTypeObjectToMap,
  jsonTypeRemap,
} from "solana-kiss-data";
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

    const targetCount = 1;

    userWitnesses.lastFew.push(witness);
    userWitnesses.lastFew.sort((a, b) => Number(b.ordering - a.ordering));
    userWitnesses.lastFew = userWitnesses.lastFew.slice(0, targetCount);

    const sampleLuck = Math.random();
    if (sampleLuck < 1 / userWitnesses.sampled.rate) {
      userWitnesses.sampled.data.push({ luck: sampleLuck, witness });
      userWitnesses.sampled.data.sort((a, b) =>
        Number(b.witness.ordering - a.witness.ordering),
      );
      while (userWitnesses.sampled.data.length >= targetCount * 1.5) {
        userWitnesses.sampled.rate *= 1.5;
        userWitnesses.sampled.data = userWitnesses.sampled.data.filter(
          (item) => item.luck < 1 / userWitnesses.sampled.rate,
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

export const coordinatorDataStoreJsonType = jsonTypeRemap(
  jsonTypeObject({
    runsInfos: jsonTypeObjectToMap(coordinatorDataRunInfoJsonType),
  }),
  (unmapped) => new CoordinatorDataStore(unmapped.runsInfos),
  (remapped) => ({ runsInfos: remapped.runsInfos }),
);
