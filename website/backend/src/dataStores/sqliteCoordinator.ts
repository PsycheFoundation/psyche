import { LLMArchitecture, PsycheCoordinator, Witness } from "psyche-deserialize-zerocopy-wasm";
import { ChainTimestamp, RunSummary, RunData, formats, CURRENT_VERSION, TxSummary, ModelType, RunStatus, OverTime, Metrics } from "shared";
import { UniqueRunKey, runKey } from "../coordinator.js";
import { CoordinatorDataStore, LastUpdateInfo } from "../dataStore.js";
import { WitnessEvalResult, WitnessMetadata } from "../idlTypes.js";
import {
    CoordinatorConfig,
    Model,
    PsycheCoordinator,
    RunMetadata,
    lr_at_step,
} from 'psyche-deserialize-zerocopy-wasm'

import Database from 'better-sqlite3'
import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { PublicKey } from "@solana/web3.js";
import { isClientWitness } from "../witness.js";
import EventEmitter from 'events';

const ALLOWLISTED_RUN_IDS =
    process.env.NODE_ENV === 'development' ? null : ['consilience-40b-1']
type Witness = Omit<WitnessMetadata, 'evals'> & {
    evals: Array<[string, number]>
}

interface RunSummaries {
    runs: RunSummary[]
    totalTokens: bigint
    totalTokensPerSecondActive: bigint
}

// TODO this is duplicated with flatFileCoordinator.ts
interface RunHistory {
    runId: string
    createdAt: ChainTimestamp
    destroyedAt: ChainTimestamp | null
    lastUpdated: ChainTimestamp

    lastState: PsycheCoordinator | null

    configChanges: Array<{
        timestamp: ChainTimestamp
        model: Model
        config: CoordinatorConfig
        metadata: RunMetadata
    }>

    trainingStep?: {
        startedAt: ChainTimestamp
        endedAt?: ChainTimestamp
        tokensCompletedAtStartOfStep: bigint
    }

    pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
    witnessUpdates: Array<[Witness, ChainTimestamp]>
    observedLrByStep: Array<[number, number]>

    recentTxs: Array<TxSummary>
}

export interface DatabaseConnection {
    db: Database.Database
    close(): void
}

function isEmptyDatabase(db: Database.Database): boolean {
    const result = db.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    ).get()
    return !result
}

export function withTransaction<T>(
    db: Database.Database,
    fn: (db: Database.Database) => T
): T {
    return db.transaction(fn)()
}

export function timestampToUnix(timestamp: { slot: BigInt; time: Date }): {
    slot: number
    time: number
} {
    return {
        slot: Number(timestamp.slot),
        time: timestamp.time.getTime(),
    }
}

export function unixToTimestamp(data: { slot: number; time: number }): {
    slot: BigInt
    time: Date
} {
    return {
        slot: BigInt(data.slot),
        time: new Date(data.time),
    }
}


function openCoordinatorDatabase(
    dbPath: string
): DatabaseConnection {
    const db = new Database(dbPath)

    // Enable foreign keys and WAL mode
    db.exec('PRAGMA foreign_keys = ON')
    db.exec('PRAGMA journal_mode = WAL')

    // Initialize schema if database is new
    if (!existsSync(dbPath) || isEmptyDatabase(db)) {
        console.log('Initializing coordinator database schema...')
        const schema = readFileSync(join(__dirname, 'coordinatorSchema.sql'), 'utf-8')
        db.exec(schema)
    }

    return {
        db,
        close: () => db.close(),
    }
}



export class SqliteCoordinatorDataStore implements CoordinatorDataStore {
    eventEmitter: EventEmitter<{ update: [UniqueRunKey] }> = new EventEmitter();

    private dbConnection: DatabaseConnection | null = null;
    private programIdString: string;
    private dbPath: string;
    #summaryCache: RunSummaries | null = null;

    constructor(dbPath: string, programId: PublicKey) {
        this.programIdString = programId.toString();
        this.dbPath = dbPath;
        this.ensureConnection();
        // TODO perhaps load all data here
    }


    private ensureConnection(): DatabaseConnection {
        if (!this.dbConnection) {
            this.dbConnection = openCoordinatorDatabase(this.dbPath)

            // Initialize sync metadata if not exists
            this.dbConnection.db.prepare(
                `INSERT OR IGNORE INTO sync_metadata (id, program_id, last_update_time, highest_signature, highest_slot) 
				 VALUES (?, ?, ?, NULL, NULL)`
            ).run(1, this.programIdString, Date.now())
        }
        return this.dbConnection
    }

    async createRun(pubkey: string, runId: string, timestamp: ChainTimestamp, newState?: PsycheCoordinator): Promise<void> {
        const connection = this.ensureConnection();
        const { slot, time } = timestampToUnix(timestamp);

        withTransaction(connection.db, (db) => {
            // Check if there's already an active run for this pubkey
            const existingRun = db.prepare(
                'SELECT id FROM runs WHERE program_id = ? AND pubkey = ? AND destroyed_at_slot IS NULL'
            ).get(this.programIdString, pubkey) as { id: number } | undefined;

            if (existingRun) {
                throw new Error(
                    `Tried to create run ${pubkey}, but we have existing run at this address`
                );
            }

            // Get next runIndex for this specific runId (for compatibility)
            const result = db.prepare(
                'SELECT MAX(run_index) as max_index FROM runs WHERE program_id = ? AND run_id = ?'
            ).get(this.programIdString, runId) as { max_index: number | null };
            const nextRunIndex = (result?.max_index ?? -1) + 1

            db.prepare(
                `INSERT INTO runs (
					program_id, run_id, run_index, pubkey, 
					created_at_slot, created_at_time, 
					last_updated_slot, last_updated_time, 
					last_state_json
				) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`
            ).run(
                this.programIdString,
                runId,
                nextRunIndex,
                pubkey,
                slot,
                time,
                slot,
                time,
                newState
                    ? JSON.stringify(newState, formats[CURRENT_VERSION].replacer)
                    : null
            )
        });

        // Clear summary cache and emit update
        this.#summaryCache = null;
    }

    async updateRun(pubkey: string, newState: PsycheCoordinator, timestamp: ChainTimestamp, configChanged: boolean): Promise<void> {
        const connection = this.ensureConnection()
        const { slot, time } = timestampToUnix(timestamp);

        withTransaction(connection.db, (db) => {
            // Find the active run for this pubkey
            const activeRun = db.prepare(
                'SELECT id, last_state_json FROM runs WHERE program_id = ? AND pubkey = ? AND destroyed_at_slot IS NULL'
            ).get(this.programIdString, pubkey) as { id: number, last_state_json: string | null } | undefined;

            if (!activeRun) {
                throw new Error(`No active run found for pubkey ${pubkey}`);
            }

            // Update the run with new state
            db.prepare(
                `UPDATE runs SET 
                    last_updated_slot = ?, last_updated_time = ?, last_state_json = ?
                 WHERE id = ?`
            ).run(
                slot,
                time,
                JSON.stringify(newState, formats[CURRENT_VERSION].replacer),
                activeRun.id
            );

            // Handle training step tracking (similar to flat file implementation)
            const lastState = activeRun.last_state_json ? JSON.parse(activeRun.last_state_json) : null;
            
            // We're entering a training step
            if (
                newState.coordinator.run_state === 'RoundTrain' &&
                (!lastState || lastState.coordinator.run_state !== 'RoundTrain')
            ) {
                const tokensCompletedAtStartOfStep = lastState ? (() => {
                    const c = lastState.coordinator;
                    const tokensPerSequence = BigInt(c.model.LLM.max_seq_len);
                    const batchSizeStart = BigInt(c.config.global_batch_size_start);
                    const batchSizeEnd = BigInt(c.config.global_batch_size_end);
                    const warmupTokens = c.config.global_batch_size_warmup_tokens;
                    const currentStep = BigInt(c.progress.step - 1);

                    return this.calculateTokens(
                        currentStep,
                        tokensPerSequence,
                        batchSizeStart,
                        batchSizeEnd,
                        warmupTokens
                    );
                })() : 0n;

                db.prepare(
                    `INSERT INTO training_steps (
                        run_id, started_at_slot, started_at_time, tokens_completed_at_start
                    ) VALUES (?, ?, ?, ?)`
                ).run(activeRun.id, slot, time, tokensCompletedAtStartOfStep.toString());
            }

            // We're leaving a training step
            if (
                newState.coordinator.run_state !== 'RoundTrain' &&
                lastState && lastState.coordinator.run_state === 'RoundTrain'
            ) {
                db.prepare(
                    `UPDATE training_steps SET ended_at_slot = ?, ended_at_time = ?
                     WHERE run_id = ? AND ended_at_slot IS NULL`
                ).run(slot, time, activeRun.id);
            }

            // Track learning rate observations
            const step = newState.coordinator.progress.step;
            const lastObservedStep = db.prepare(
                'SELECT MAX(step) as max_step FROM lr_observations WHERE run_id = ?'
            ).get(activeRun.id) as { max_step: number | null };

            if (step > (lastObservedStep?.max_step ?? 0)) {
                // Calculate learning rate using lr_at_step function
                const lr = lr_at_step(newState.coordinator.model.LLM.lr_schedule, step);
                db.prepare(
                    'INSERT INTO lr_observations (run_id, step, learning_rate) VALUES (?, ?, ?)'
                ).run(activeRun.id, step, lr);
            }

            // Track config changes
            if (configChanged) {
                db.prepare(
                    `INSERT INTO config_changes (
                        run_id, timestamp_slot, timestamp_time, 
                        model_json, config_json, metadata_json
                    ) VALUES (?, ?, ?, ?, ?, ?)`
                ).run(
                    activeRun.id,
                    slot,
                    time,
                    JSON.stringify(newState.coordinator.model),
                    JSON.stringify(newState.coordinator.config),
                    JSON.stringify(newState.metadata)
                );
            }
        });

        // Clear summary cache
        this.#summaryCache = null;
    }

    async setRunPaused(pubkey: string, paused: boolean, timestamp: ChainTimestamp): Promise<void> {
        const connection = this.ensureConnection();
        const newPauseState = paused ? 'paused' : 'unpaused';

        const result = connection.db.prepare(`
            INSERT INTO pause_events (run_id, event_type, timestamp_slot, timestamp_time)
            SELECT r.id, ?, ?, ?
            FROM runs r 
            WHERE r.pubkey = ? AND r.destroyed_at_slot IS NULL
        `).run(newPauseState, timestamp.slot, timestamp.time, pubkey);

        if (result.changes === 0) {
            throw new Error(`No active run found for pubkey ${pubkey}`);
        }

        // Clear summary cache
        this.#summaryCache = null;
    }

    async witnessRun(pubkey: string, witness: WitnessMetadata, timestamp: ChainTimestamp): Promise<void> {
        const connection = this.ensureConnection();

        const { evals, ...restWitness } = witness;

        const l =
            typeof evals.len === 'object' && evals.len && 'toNumber' in evals.len
                ? evals.len.toNumber()
                : Number(evals.len);
        const fixedEvals: Array<[string, number]> = [];
        for (const { name, value } of evals.data.slice(
            0,
            l
        ) as WitnessEvalResult[]) {
            const firstZero = name[0].findIndex((v) => v === 0)
            const nameStr = Buffer.from(name[0].slice(0, firstZero)).toString('utf-8')
            fixedEvals.push([nameStr, value])
        };

        const result = connection.db.prepare(
            `INSERT INTO witness_updates (
                run_id, witness_json, timestamp_slot, timestamp_time,
                step, tokens_per_sec, bandwidth_per_sec, loss, efficiency, evals
            )
            SELECT r.id, ?, ?, ?, ?, ?, ?, ?, ?, ?
            FROM runs r 
            WHERE r.pubkey = ? AND r.destroyed_at_slot IS NULL`
        ).run(
            JSON.stringify({ ...restWitness, evals: fixedEvals }),
            timestamp.slot,
            timestamp.time,
            restWitness.step,
            restWitness.tokens_per_sec,
            restWitness.bandwidth_per_sec,
            restWitness.loss,
            restWitness.efficiency,
            JSON.stringify(fixedEvals),
            pubkey
        );

        if (result.changes === 0) {
            throw new Error(`No active run found for pubkey ${pubkey}`);
        }

        // Clear summary cache
        this.#summaryCache = null;
    }

    async destroyRun(pubkey: string, timestamp: ChainTimestamp): Promise<void> {
        const connection = this.ensureConnection();
        
        const result = connection.db.prepare(
            `UPDATE runs SET destroyed_at_slot = ?, destroyed_at_time = ? 
             WHERE program_id = ? AND pubkey = ? AND destroyed_at_slot IS NULL`
        ).run(
            timestamp.slot,
            timestamp.time,
            this.programIdString,
            pubkey
        );

        if (result.changes === 0) {
            throw new Error(`No active run found for pubkey ${pubkey}`);
        }

        // Clear summary cache
        this.#summaryCache = null;
    }

    async trackTx(runPubkey: string, userPubkey: string, method: string, data: string, txHash: string, timestamp: ChainTimestamp): Promise<void> {
        const connection = this.ensureConnection();

        const result = connection.db.prepare(`
            INSERT INTO transactions (
            run_id, user_pubkey, method, data, tx_hash, timestamp_slot, timestamp_time
            )
            SELECT r.id, ?, ?, ?, ?, ?, ?
            FROM runs r 
            WHERE r.pubkey = ? AND r.destroyed_at_slot IS NULL
        `).run(
            userPubkey,
            method,
            data,
            txHash,
            timestamp.slot,
            timestamp.time,
            runPubkey
        );

        if (result.changes === 0) {
            throw new Error(`No active run found for pubkey ${runPubkey}`);
        }

        // Clear summary cache
        this.#summaryCache = null;
    }

    async getRunSummaries(): Promise<RunSummaries> {
        if (this.#summaryCache) {
            return this.#summaryCache;
        }

        const connection = this.ensureConnection();

        // Get all runs with their computed summaries
        const runs = connection.db.prepare(`
    WITH run_data AS (
      SELECT 
        r.id,
        r.program_id,
        r.run_id,
        r.run_index,
        r.pubkey,
        r.created_at_slot,
        r.created_at_time,
        r.destroyed_at_slot,
        r.destroyed_at_time,
        r.last_updated_slot,
        r.last_updated_time,
        r.last_state_json,
        
        -- Get latest training step
        ts.tokens_completed_at_start,
        ts.started_at_slot as training_step_started_slot,
        ts.started_at_time as training_step_started_time,
        ts.ended_at_slot as training_step_ended_slot,
        ts.ended_at_time as training_step_ended_time,
        
        -- Get current pause state
        pe_latest.event_type as current_pause_state,
        
        -- Check if this is the only run with state for this run_id
        COUNT(*) OVER (
          PARTITION BY r.run_id 
          WHERE r.last_state_json IS NOT NULL
        ) as runs_with_state_count,
        
        -- Get latest witness update for tokens per second
        wu.tokens_per_sec as last_tokens_per_sec
        
      FROM runs r
      
      -- Latest training step
      LEFT JOIN (
        SELECT 
          run_id,
          tokens_completed_at_start,
          started_at_slot,
          started_at_time,
          ended_at_slot,
          ended_at_time,
          ROW_NUMBER() OVER (PARTITION BY run_id ORDER BY started_at_slot DESC) as rn
        FROM training_steps
      ) ts ON r.id = ts.run_id AND ts.rn = 1
      
      -- Latest pause event
      LEFT JOIN (
        SELECT 
          run_id,
          event_type,
          ROW_NUMBER() OVER (PARTITION BY run_id ORDER BY timestamp_slot DESC) as rn
        FROM pause_events
      ) pe_latest ON r.id = pe_latest.run_id AND pe_latest.rn = 1
      
      -- Latest witness update for tokens per second
      LEFT JOIN (
        SELECT 
          run_id,
          tokens_per_sec,
          ROW_NUMBER() OVER (PARTITION BY run_id ORDER BY timestamp_slot DESC) as rn
        FROM witness_updates
      ) wu ON r.id = wu.run_id AND wu.rn = 1
      
      WHERE r.last_state_json IS NOT NULL
        ${ALLOWLISTED_RUN_IDS ?
                `AND r.run_id IN (${ALLOWLISTED_RUN_IDS.map(() => '?').join(',')})` :
                ''
            }
    )
    
    SELECT 
      run_id as id,
      run_index,
      last_updated_slot,
      last_updated_time,
      destroyed_at_slot,
      destroyed_at_time,
      last_state_json,
      tokens_completed_at_start,
      training_step_started_slot,
      training_step_started_time,
      training_step_ended_slot,
      training_step_ended_time,
      last_tokens_per_sec,
      
      -- Determine status
      CASE 
        WHEN destroyed_at_slot IS NOT NULL THEN 'destroyed'
        WHEN current_pause_state = 'paused' THEN 'paused'
        ELSE 'active'
      END as status_type,
      
      -- Check if this is the only run at this index
      CASE WHEN runs_with_state_count = 1 THEN 1 ELSE 0 END as is_only_run_at_index
      
    FROM run_data
    ORDER BY created_at_time DESC
  `).all(...(ALLOWLISTED_RUN_IDS ? [...ALLOWLISTED_RUN_IDS] : []));

        // Get pause history for all runs
        const pauseHistories = connection.db.prepare(`
    SELECT 
      r.run_id,
      pe.event_type,
      pe.timestamp_slot,
      pe.timestamp_time
    FROM runs r
    JOIN pause_events pe ON r.id = pe.run_id
    WHERE r.last_state_json IS NOT NULL
      ${ALLOWLISTED_RUN_IDS ?
                `AND r.run_id IN (${ALLOWLISTED_RUN_IDS.map(() => '?').join(',')})` :
                ''
            }
    ORDER BY r.run_id, pe.timestamp_slot
  `).all(...(ALLOWLISTED_RUN_IDS || []));

        // Group pause history by run_id
        const pauseHistoryMap = new Map<string, Array<['paused' | 'unpaused', ChainTimestamp]>>();
        for (const row of pauseHistories) {
            if (!pauseHistoryMap.has(row.run_id)) {
                pauseHistoryMap.set(row.run_id, []);
            }
            pauseHistoryMap.get(row.run_id)!.push([
                row.event_type as 'paused' | 'unpaused',
                { slot: row.timestamp_slot, time: row.timestamp_time }
            ]);
        }

        // Transform SQL results to RunSummary objects
        const runSummaries: RunSummary[] = runs.map(row => {
            // Parse the last_state_json to extract additional properties
            const lastState = row.last_state_json ? JSON.parse(row.last_state_json) : {};
            const metadata = lastState.metadata || {};
            const coordinator = lastState.coordinator || {};
            const model = coordinator.model || {};

            return {
                id: row.id,
                index: row.run_index,
                isOnlyRunAtThisIndex: row.is_only_run_at_index === 1,
                name: metadata.name || '',
                description: metadata.description || '',

                status: {
                    type: row.status_type as 'active' | 'paused' | 'destroyed'
                } as RunStatus,

                pauseHistory: pauseHistoryMap.get(row.id) || [],

                totalTokens: BigInt(row.tokens_completed_at_start || 0),

                lastUpdate: {
                    slot: row.last_updated_slot,
                    time: row.last_updated_time
                } as ChainTimestamp,

                trainingStep: row.tokens_completed_at_start ? {
                    lastTokensPerSecond: BigInt(row.last_tokens_per_sec || 0),
                    startedAt: {
                        slot: row.training_step_started_slot,
                        time: row.training_step_started_time
                    } as ChainTimestamp,
                    endedAt: row.training_step_ended_slot ? {
                        slot: row.training_step_ended_slot,
                        time: row.training_step_ended_time
                    } as ChainTimestamp : undefined,
                    tokensCompletedAtStartOfStep: BigInt(row.tokens_completed_at_start)
                } : undefined,

                // Extract from model configuration in JSON
                size: BigInt(metadata.num_parameters || 0),
                arch: model.architecture || 'unknown' as LLMArchitecture,
                type: model.data_type || 'unknown' as ModelType
            };
        });

        // Calculate totals
        const totalTokens = runSummaries.reduce(
            (sum, run) => sum + (run.trainingStep?.tokensCompletedAtStartOfStep ?? 0n),
            0n
        );

        const totalTokensPerSecondActive = runSummaries.reduce((sum, summary) => {
            if (summary.status.type !== 'active') { //|| !summary.isRecentlyActive) {
                return sum;
            }
            return sum + (summary.trainingStep?.tokensCompletedAtStartOfStep ?? 0n);
        }, 0n);

        const summaries: RunSummaries = {
            runs: runSummaries,
            totalTokens,
            totalTokensPerSecondActive
        };

        this.#summaryCache = summaries;
        return summaries;
    }

    async getRunDataById(runId: string, index: number): Promise<RunData | null> {
        const connection = await this.ensureConnection();

        // Get the basic run info
        const run = await connection.db.get(`
    SELECT 
      r.id,
      r.run_id,
      r.run_index,
      r.pubkey,
      r.created_at_slot,
      r.created_at_time,
      r.destroyed_at_slot,
      r.destroyed_at_time,
      r.last_updated_slot,
      r.last_updated_time,
      r.last_state_json,
      
      -- Count runs with state for isOnlyRunAtThisIndex
      (SELECT COUNT(*) 
       FROM runs r2 
       WHERE r2.run_id = r.run_id 
         AND r2.last_state_json IS NOT NULL) as runs_with_state_count
         
    FROM runs r
    WHERE r.run_id = ? AND r.run_index = ?
  `, [runId, index]);

        if (!run) {
            return null;
        }

        // Get witness updates for this run
        const witnessUpdates = await connection.db.all(`
    SELECT 
      step,
      tokens_per_sec,
      bandwidth_per_sec,
      loss,
      efficiency,
      timestamp_slot,
      timestamp_time,
      evals
    FROM witness_updates
    WHERE run_id = ?
    ORDER BY step ASC
  `, [run.id]);

        // Get learning rate observations
        const lrObservations = await connection.db.all(`
    SELECT step, learning_rate
    FROM lr_observations
    WHERE run_id = ?
    ORDER BY step ASC
  `, [run.id]);

        // Get recent transactions
        const recentTxs = await connection.db.all(`
    SELECT 
      user_pubkey as pubkey,
      data,
      method,
      timestamp_slot,
      timestamp_time,
      tx_hash as txHash
    FROM transactions
    WHERE run_id = ?
    ORDER BY timestamp_slot DESC
    LIMIT 100
  `, [run.id]);

        // Get pause history for this specific run
        const pauseHistory = await connection.db.all(`
            SELECT event_type, timestamp_slot, timestamp_time
            FROM pause_events
            WHERE run_id = ?
            ORDER BY timestamp_slot ASC
        `, [run.id]);

        // Get training step info
        const trainingStep = await connection.db.get(`
            SELECT 
                tokens_completed_at_start,
                started_at_slot,
                started_at_time,
                ended_at_slot,
                ended_at_time
            FROM training_steps
            WHERE run_id = ?
            ORDER BY started_at_slot DESC
            LIMIT 1
        `, [run.id]);

        // Get latest witness update for tokens per second
        const latestWitness = await connection.db.get(`
            SELECT tokens_per_sec
            FROM witness_updates
            WHERE run_id = ?
            ORDER BY timestamp_slot DESC
            LIMIT 1
        `, [run.id]);

        // Create the run summary info
        const lastState = run.last_state_json ? JSON.parse(run.last_state_json) : null;
        const metadata = lastState?.metadata || {};
        const coordinator = lastState?.coordinator || {};
        const model = coordinator.model || {};

        // Determine status
        let status: RunStatus;
        if (run.destroyed_at_slot) {
            status = { type: 'completed', at: { slot: BigInt(run.destroyed_at_slot), time: new Date(run.destroyed_at_time) } };
        } else {
            // Check latest pause state
            const latestPause = pauseHistory.at(-1);
            if (latestPause?.event_type === 'paused') {
                status = { type: 'paused' };
            } else {
                status = { type: 'active' };
            }
        }

        const info: RunSummary = {
            id: run.run_id,
            index: run.run_index,
            isOnlyRunAtThisIndex: run.runs_with_state_count === 1,
            name: metadata.name || '',
            description: metadata.description || '',

            status,

            pauseHistory: pauseHistory.map(p => [
                p.event_type as 'paused' | 'unpaused',
                { slot: BigInt(p.timestamp_slot), time: new Date(p.timestamp_time) }
            ]),

            totalTokens: BigInt(trainingStep?.tokens_completed_at_start || 0),

            lastUpdate: {
                slot: BigInt(run.last_updated_slot),
                time: new Date(run.last_updated_time)
            },

            trainingStep: trainingStep ? {
                lastTokensPerSecond: BigInt(latestWitness?.tokens_per_sec || 0),
                startedAt: {
                    slot: BigInt(trainingStep.started_at_slot),
                    time: new Date(trainingStep.started_at_time)
                },
                endedAt: trainingStep.ended_at_slot ? {
                    slot: BigInt(trainingStep.ended_at_slot),
                    time: new Date(trainingStep.ended_at_time)
                } : undefined,
                tokensCompletedAtStartOfStep: BigInt(trainingStep.tokens_completed_at_start)
            } : undefined,

            size: BigInt(metadata.num_parameters || 0),
            arch: model.architecture || 'unknown' as LLMArchitecture,
            type: model.data_type || 'text' as ModelType
        };

        // Process witness updates into history data
        const numSamples = 1000;

        // Parse evals from JSON and organize by step
        const evalsByStep = new Map<number, Record<string, number>>();
        const bandwidthData: Array<[number, number]> = [];
        const lossData: Array<[number, number]> = [];
        const tokensPerSecData: Array<[number, number]> = [];

        for (const update of witnessUpdates) {
            const step = update.step;

            // Add basic metrics
            if (update.bandwidth_per_sec != null) {
                bandwidthData.push([step, update.bandwidth_per_sec]);
            }
            if (update.loss != null) {
                lossData.push([step, update.loss]);
            }
            if (update.tokens_per_sec != null) {
                tokensPerSecData.push([step, update.tokens_per_sec]);
            }

            // Parse evals JSON
            if (update.evals) {
                try {
                    const evals = JSON.parse(update.evals);
                    if (Array.isArray(evals?.data)) {
                        const evalRecord: Record<string, number> = {};
                        for (const evalItem of evals.data) {
                            if (evalItem.name && evalItem.value != null) {
                                // Convert name array to string if needed
                                const name = Array.isArray(evalItem.name) ?
                                    evalItem.name.join('') :
                                    evalItem.name.toString();
                                evalRecord[name] = evalItem.value;
                            }
                        }
                        evalsByStep.set(step, evalRecord);
                    }
                } catch (e) {
                    // Skip invalid JSON
                }
            }
        }

        // Organize evals by name
        const evals: Record<string, Array<[number, number]>> = {};
        for (const [step, stepEvals] of evalsByStep) {
            for (const [name, value] of Object.entries(stepEvals)) {
                if (!(name in evals)) {
                    evals[name] = [];
                }
                evals[name].push([step, value]);
            }
        }

        // Apply sampling and averaging (simplified versions of fairSample and averageSameStepValues)
        const sampleData = (data: Array<[number, number]>) => {
            if (data.length <= numSamples) return data;
            const step = Math.floor(data.length / numSamples);
            return data.filter((_, i) => i % step === 0).slice(0, numSamples);
        };

        const history: OverTime<Metrics> = {
            bandwidth: sampleData(bandwidthData),
            loss: sampleData(lossData),
            tokensPerSecond: sampleData(tokensPerSecData),
            lr: lrObservations.map(obs => [obs.step, obs.learning_rate] as [number, number]),
            evals: Object.fromEntries(
                Object.entries(evals).map(([name, data]) => [name, sampleData(data)])
            ),
        };

        // Create current metrics summary
        const summary: Metrics = {
            bandwidth: bandwidthData.at(-1)?.[1] ?? 0,
            loss: lossData.at(-1)?.[1] ?? Infinity,
            tokensPerSecond: tokensPerSecData.at(-1)?.[1] ?? 0,
            lr: lrObservations.at(-1)?.learning_rate ?? 0,
            evals: Object.fromEntries(
                Object.entries(evals)
                    .map(([k, v]) => [k, v.at(-1)?.[1]] as const)
                    .filter((x): x is [string, number] => x[1] !== undefined)
            ),
        };

        // Build state from last_state_json
        let state: RunData['state'] = undefined;

        if (lastState) {
            const c = lastState;
            const clients = c.coordinator?.epoch_state?.clients || [];
            const currentRound = c.coordinator?.epoch_state?.rounds?.[c.coordinator.epoch_state.rounds_head];

            if (currentRound) {
                const witnessStates = clients.map((client: any, index: number) => {
                    const isWitness = this.isClientWitness(
                        index,
                        currentRound.random_seed,
                        clients.length,
                        c.coordinator.config.witness_nodes
                    );
                    const witnessStatus = isWitness
                        ? currentRound.witnesses.some((w: any) => Number(w.proof.index) === index)
                            ? 'done'
                            : 'waiting'
                        : false;

                    return {
                        pubkey: client.id.signer, // Adjust based on your PublicKey handling
                        witness: witnessStatus,
                    };
                });

                const checkpoint = this.extractCheckpoint(c.coordinator.model.LLM.checkpoint);
                const config = c.coordinator.config;

                state = {
                    phase: c.coordinator.run_state,
                    phaseStartTime: new Date(Number(`${c.coordinator.run_state_start_unix_timestamp}000`)),
                    round: currentRound.height,
                    clients: witnessStates,
                    checkpoint,
                    config: {
                        minClients: config.init_min_clients,
                        roundsPerEpoch: config.rounds_per_epoch,
                        cooldownTime: Number(config.cooldown_time),
                        maxRoundTrainTime: Number(config.max_round_train_time),
                        roundWitnessTime: Number(config.round_witness_time),
                        warmupTime: Number(config.warmup_time),
                        lrSchedule: c.coordinator.model.LLM.lr_schedule,
                    },
                };
            }
        }

        return {
            info,
            state,
            recentTxs: recentTxs.map(tx => ({
                pubkey: tx.pubkey,
                data: tx.data,
                method: tx.method,
                timestamp: {
                    slot: tx.timestamp_slot,
                    time: tx.timestamp_time
                },
                txHash: tx.txHash
            })),
            metrics: {
                summary,
                history,
            },
        };
    }

    // Helper methods you'll need to implement
    private isClientWitness(index: number, randomSeed: bigint, clientsLength: number, witnessNodes: number): boolean {
        // Implement your witness selection logic
        // This should match your existing isClientWitness function
        return false; // placeholder
    }
    
    private extractCheckpoint(checkpoint: any): any {
        // Extract checkpoint info from the JSON structure
        if (typeof checkpoint === 'object') {
            if ('Hub' in checkpoint) return checkpoint.Hub;
            if ('P2P' in checkpoint) return checkpoint.P2P;
        }
        return null;
    }

    /**
     * The warmup function is actually exponential,
     * since it's based on its own output from the previous step,
     * and transitions to linear after a specific tokens threshold.
     * This is annoying to model, so we just do the recursive calc.
     */
    private calculateTokens(
        step: bigint,
        tokensPerSequence: bigint,
        batchSizeStart: bigint,
        batchSizeEnd: bigint,
        warmupTokens: bigint
    ): bigint {
        let currentDataIndex = 0n;

        for (let i = 0n; i < step; i++) {
            const tokensProcessedBeforeStep = currentDataIndex * tokensPerSequence;

            let batchSizeForStep: bigint;
            if (tokensProcessedBeforeStep >= warmupTokens) {
                batchSizeForStep = batchSizeEnd;
            } else {
                const progress = Number(tokensProcessedBeforeStep) / Number(warmupTokens);
                const batchSize =
                    Number(batchSizeStart) +
                    (Number(batchSizeEnd) - Number(batchSizeStart)) * progress;
                batchSizeForStep = BigInt(Math.round(batchSize));
            }

            currentDataIndex += batchSizeForStep;
        }

        return currentDataIndex * tokensPerSequence;
    }

    async getNumRuns(): Promise<number> {
        const connection = await this.ensureConnection();

        const result = await connection.db.get(`
            SELECT COUNT(*) as count
            FROM runs r
            WHERE r.last_state_json IS NOT NULL
            ${ALLOWLISTED_RUN_IDS ?
                `AND r.run_id IN (${ALLOWLISTED_RUN_IDS.map(() => '?').join(',')})` :
                ''
            }
        `, ALLOWLISTED_RUN_IDS || []);

        return result.count;
    }


    async lastUpdate(): Promise<LastUpdateInfo> {
        const connection = await this.ensureConnection();

        const result = await connection.db.get(`
            SELECT 
            last_update_time as time,
            highest_signature,
            highest_slot
            FROM sync_metadata
            WHERE id = 1
        `);

        if (!result) {
            // Return default if no sync metadata exists yet
            return {
                time: new Date(),
                highestSignature: undefined,
            };
        }

        return {
            time: new Date(result.time),
            highestSignature: result.highest_signature || undefined,
        };
    }


    async sync(lastUpdateInfo: LastUpdateInfo): Promise<void> {
        const connection = await this.ensureConnection();
        
        await connection.db.run(`
            UPDATE sync_metadata 
            SET last_update_time = ?, highest_signature = ?, highest_slot = ?
            WHERE id = 1
        `, [
            lastUpdateInfo.time.getTime(),
            lastUpdateInfo.highestSignature?.signature || null,
            lastUpdateInfo.highestSignature?.slot || null
        ]);

        // Clear summary cache
        this.#summaryCache = null;
    }


}