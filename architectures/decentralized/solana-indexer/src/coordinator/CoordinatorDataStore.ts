import { Immutable } from "../utils";

export interface CoordinatorDataStoreRunAccount {
  runId: string;
}

export interface CoordinatorDataStoreRun {
  latestAccountState: CoordinatorDataStoreRunAccount | undefined;
  latestAccountOrdering: bigint;
}

export class CoordinatorDataStore {
  private runs: Map<string, CoordinatorDataStoreRun>;

  constructor(runs: Map<string, CoordinatorDataStoreRun>) {
    this.runs = runs;
  }

  public getRuns(): Immutable<Map<string, CoordinatorDataStoreRun>> {
    return this.runs;
  }

  public saveRunAccountState(
    runAddress: string,
    accountState: CoordinatorDataStoreRunAccount,
  ) {
    let run = this.runs.get(runAddress);
    if (run != undefined) {
      run.latestAccountState = accountState;
    } else {
      run = {
        latestAccountState: accountState,
        latestAccountOrdering: 0n,
      };
      this.runs.set(runAddress, run);
    }
  }

  public invalidateRunAccountState(runAddress: string, ordering: bigint) {
    const run = this.runs.get(runAddress);
    if (run === undefined) {
      return;
    }
    if (ordering > run.latestAccountOrdering) {
      run.latestAccountState = undefined;
      run.latestAccountOrdering = ordering;
    }
  }

  public getInvalidatedRunsAddresses(): Array<string> {
    const dirtyRuns: Array<string> = [];
    for (const [runAddress, run] of this.runs.entries()) {
      if (run.latestAccountState === undefined) {
        dirtyRuns.push(runAddress);
      }
    }
    return dirtyRuns;
  }
}
