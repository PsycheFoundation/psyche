import { readFileSync } from 'fs'

import path from 'path'
import { psycheJsonReviver, psycheJsonReplacer, ContributionInfo } from 'shared'
import { LastUpdateInfo, MiningPoolDataStore } from '../dataStore.js'
import { PsycheMiningPoolAccount } from '../idlTypes.js'
import { PublicKey } from '@solana/web3.js'
import EventEmitter from 'events'
import { writeFileAtomic } from '../writeFileAtomic.js'

export class FlatFileMiningPoolDataStore implements MiningPoolDataStore {
	#lastUpdateInfo: LastUpdateInfo = {
		time: new Date(),
		highestSignature: undefined,
	}
	#programId: PublicKey
	#data: {
		totalDepositedCollateralAmount: bigint
		maxDepositCollateralAmount: bigint
		collateral: {
			mintAddress: string
			decimals: number
		} | null
		userDeposits: Map<string, bigint>
	} = {
		collateral: null,
		maxDepositCollateralAmount: 0n,
		totalDepositedCollateralAmount: 0n,
		userDeposits: new Map(),
	}
	#db: string

	eventEmitter: EventEmitter<{ update: [] }> = new EventEmitter()

	constructor(dir: string, programId: PublicKey) {
		this.#db = path.join(dir, `./mining-pool-db-${programId}.json`)
		this.#programId = programId

		console.log(`loading mining pool db from disk at path ${this.#db}...`)
		try {
			const { lastUpdateInfo, data, programId } = JSON.parse(
				readFileSync(this.#db, 'utf-8'),
				psycheJsonReviver
			)
			if (this.#programId.equals(programId)) {
				this.#lastUpdateInfo = lastUpdateInfo
				this.#data = data
				console.log(
					`loaded DB from disk. previous info state: time: ${this.#lastUpdateInfo.time}, ${JSON.stringify(this.#lastUpdateInfo.highestSignature)}`
				)
			} else {
				console.warn(
					`Program ID for mining pool changed from ${programId} in saved state to ${this.#programId} in args. **Starting from a fresh database**.`
				)
			}
		} catch (err) {
			console.warn('failed to load previous DB from disk: ', err)
		}
	}

	setFundingData(data: PsycheMiningPoolAccount): void {
		this.#data.maxDepositCollateralAmount = BigInt(
			data.maxDepositCollateralAmount.toString()
		)
		this.#data.totalDepositedCollateralAmount = BigInt(
			data.totalDepositedCollateralAmount.toString()
		)
	}

	setCollateralInfo(mintAddress: string, decimals: number) {
		this.#data.collateral = {
			mintAddress,
			decimals,
		}
	}

	setUserAmount(address: string, amount: bigint): void {
		this.#data.userDeposits.set(address, amount)
	}

	lastUpdate(): LastUpdateInfo {
		return this.#lastUpdateInfo
	}

	async sync(lastUpdateInfo: LastUpdateInfo): Promise<void> {
		this.#lastUpdateInfo = lastUpdateInfo
		await writeFileAtomic(
			this.#db,
			JSON.stringify(
				{
					lastUpdateInfo: this.#lastUpdateInfo,
					data: this.#data,
					programId: this.#programId,
				},
				psycheJsonReplacer
			)
		)
		this.eventEmitter.emit('update')
	}

	getContributionInfo(): Omit<ContributionInfo, 'miningPoolProgramId'> {
		const usersSortedByAmount = [...this.#data.userDeposits.entries()].sort(
			(a, b) => (a[1] > b[1] ? -1 : a[1] < b[1] ? 1 : 0)
		)
		return {
			totalDepositedCollateralAmount: this.#data.totalDepositedCollateralAmount,
			maxDepositCollateralAmount: this.#data.maxDepositCollateralAmount,
			users: usersSortedByAmount.map(([address, funding], i) => ({
				address,
				funding,
				rank: i + 1,
			})),
			collateralMintDecimals: this.#data.collateral?.decimals ?? 0,
			collateralMintAddress: this.#data.collateral?.mintAddress ?? 'UNKNOWN',
		}
	}
	hasCollateralInfo(): boolean {
		return this.#data.collateral !== null
	}
}
