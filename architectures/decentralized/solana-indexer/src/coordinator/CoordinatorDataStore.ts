import {
  JsonType,
  jsonTypeObject,
  jsonTypeRemap,
  JsonValue,
  Pubkey,
} from "solana-kiss";
import { utilsObjectToPubkeyMapJsonType } from "../utils";
import {
  CoordinatorDataRunInfo,
  coordinatorDataRunInfoJsonType,
} from "./CoordinatorDataRunInfo";
import { CoordinatorDataRunState } from "./CoordinatorDataRunState";

export class CoordinatorDataStore {
  public runsInfos: Map<Pubkey, CoordinatorDataRunInfo>;

  constructor(runs: Map<Pubkey, CoordinatorDataRunInfo>) {
    this.runsInfos = runs;
  }

  public getRunInfo(runAddress: Pubkey): CoordinatorDataRunInfo {
    let runInfo = this.runsInfos.get(runAddress);
    if (runInfo === undefined) {
      runInfo = {
        accountState: undefined,
        accountUpdatedAt: undefined,
        accountFetchedOrdering: 0n,
        accountRequestOrdering: 0n,
        witnessesPerUser: new Map(),
        adminHistory: [],
      };
      this.runsInfos.set(runAddress, runInfo);
    }
    return runInfo;
  }

  public saveRunState(runAddress: Pubkey, runState: CoordinatorDataRunState) {
    let runInfo = this.getRunInfo(runAddress);
    runInfo.accountState = runState;
    runInfo.accountUpdatedAt = new Date();
    runInfo.accountFetchedOrdering = runInfo.accountRequestOrdering;
  }

  public saveRunWitness(
    runAddress: Pubkey,
    signerAddress: Pubkey,
    ordering: bigint,
    processedTime: Date | undefined,
    proof: {
      position: bigint;
      index: bigint;
      witness: boolean;
    },
    metadata: {
      tokensPerSec: number;
      bandwidthPerSec: number;
      loss: number;
      step: number;
    },
  ) {
    const runInfo = this.getRunInfo(runAddress);
    const userWitnesses = runInfo.witnessesPerUser.get(signerAddress) ?? {
      lastFew: [],
      sampled: { rate: 1, data: [] },
    };
    const desiredLastFewCount = 1;
    const desiredSampledCount = 5;
    const witness = { processedTime, ordering, proof, metadata };
    userWitnesses.lastFew.push(witness);
    userWitnesses.lastFew.sort((a, b) => Number(b.ordering - a.ordering));
    userWitnesses.lastFew = userWitnesses.lastFew.slice(0, desiredLastFewCount);
    const selector = Math.random();
    if (selector < 1 / userWitnesses.sampled.rate) {
      userWitnesses.sampled.data.push({ selector, witness });
      userWitnesses.sampled.data.sort((a, b) =>
        Number(b.witness.ordering - a.witness.ordering),
      );
      while (userWitnesses.sampled.data.length >= desiredSampledCount * 1.5) {
        userWitnesses.sampled.rate *= 1.5;
        userWitnesses.sampled.data = userWitnesses.sampled.data.filter(
          (item) => item.selector < 1 / userWitnesses.sampled.rate,
        );
      }
    }
    runInfo.witnessesPerUser.set(signerAddress, userWitnesses);
  }

  public saveRunAdminAction(
    runAddress: Pubkey,
    signerAddress: Pubkey,
    instructionName: string,
    instructionAddresses: Map<string, Pubkey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    processedTime: Date | undefined,
  ) {
    const runInfo = this.getRunInfo(runAddress);
    runInfo.adminHistory.push({
      processedTime,
      ordering,
      signerAddress,
      instructionName,
      instructionAddresses,
      instructionPayload,
    });
    runInfo.adminHistory.sort((a, b) => Number(b.ordering - a.ordering));
  }

  public setRunRequestOrdering(runAddress: Pubkey, ordering: bigint) {
    const runInfo = this.getRunInfo(runAddress);
    if (ordering > runInfo.accountRequestOrdering) {
      runInfo.accountRequestOrdering = ordering;
    }
  }
}

export const coordinatorDataStoreJsonType: JsonType<CoordinatorDataStore> =
  jsonTypeRemap(
    jsonTypeObject((key) => key, {
      runsInfos: utilsObjectToPubkeyMapJsonType(coordinatorDataRunInfoJsonType),
    }),
    (unmapped) => new CoordinatorDataStore(unmapped.runsInfos),
    (remapped) => ({ runsInfos: remapped.runsInfos }),
  );
