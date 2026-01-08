import {
	JsonCodec,
	jsonCodecObject,
	jsonCodecPubkey,
	jsonCodecTransform,
	Pubkey,
	pubkeyFindPdaAddress,
	utf8Encode,
} from 'solana-kiss'
import { utilsObjectToPubkeyMapJsonCodec } from '../utils'
import {
	CoordinatorDataRunInfo,
	coordinatorDataRunInfoJsonCodec,
} from './CoordinatorDataRunInfo'

export class CoordinatorDataStore {
	public programAddress: Pubkey
	public runInfoByAddress: Map<Pubkey, CoordinatorDataRunInfo>

	constructor(
		programAddress: Pubkey,
		runInfoByAddress: Map<Pubkey, CoordinatorDataRunInfo>
	) {
		this.programAddress = programAddress
		this.runInfoByAddress = runInfoByAddress
	}

	public getRunAddress(runId: string): Pubkey {
		const runIdSeed = new Uint8Array(32)
		runIdSeed.set(utf8Encode(runId).slice(0, 32))
		return pubkeyFindPdaAddress(this.programAddress, [
			utf8Encode('coordinator'),
			runIdSeed,
		])
	}

	public getRunInfo(runAddress: Pubkey): CoordinatorDataRunInfo {
		let runInfo = this.runInfoByAddress.get(runAddress)
		if (runInfo === undefined) {
			runInfo = {
				accountState: undefined,
				accountUpdatedAt: undefined,
				changeAcknowledgedOrdinal: 0n,
				changeNotificationOrdinal: 0n,
				lastWitnessByUser: new Map(),
				samplesByStatName: new Map(),
				finishesOrdinals: [],
				importantHistory: [],
			}
			this.runInfoByAddress.set(runAddress, runInfo)
		}
		return runInfo
	}
}

export const coordinatorDataStoreJsonCodec: JsonCodec<CoordinatorDataStore> =
	jsonCodecTransform(
		jsonCodecObject({
			programAddress: jsonCodecPubkey,
			runInfoByAddress: utilsObjectToPubkeyMapJsonCodec(
				coordinatorDataRunInfoJsonCodec
			),
		}),
		{
			decoder: (encoded) =>
				new CoordinatorDataStore(
					encoded.programAddress,
					encoded.runInfoByAddress
				),
			encoder: (decoded) => ({
				programAddress: decoded.programAddress,
				runInfoByAddress: decoded.runInfoByAddress,
			}),
		}
	)
