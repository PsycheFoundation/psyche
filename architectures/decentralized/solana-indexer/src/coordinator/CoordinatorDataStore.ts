import {
	JsonCodec,
	jsonCodecObject,
	jsonCodecObjectToMap,
	jsonCodecPubkey,
	jsonCodecTransform,
	Pubkey,
} from 'solana-kiss'
import { utilsObjectToPubkeyMapJsonCodec } from '../utils'
import {
	CoordinatorDataRunInfo,
	coordinatorDataRunInfoJsonCodec,
} from './CoordinatorDataRunInfo'
import { CoordinatorDataRunState } from './CoordinatorDataRunState'

export class CoordinatorDataStore {
	public runAddressByRunId: Map<string, Pubkey>
	public runInfoByAddress: Map<Pubkey, CoordinatorDataRunInfo>

	constructor(
		runAddressByRunId: Map<string, Pubkey>,
		runInfoByAddress: Map<Pubkey, CoordinatorDataRunInfo>
	) {
		this.runAddressByRunId = runAddressByRunId
		this.runInfoByAddress = runInfoByAddress
	}

	public getRunInfo(runAddress: Pubkey): CoordinatorDataRunInfo {
		let runInfo = this.runInfoByAddress.get(runAddress)
		if (runInfo === undefined) {
			runInfo = {
				accountState: undefined,
				accountUpdatedAt: undefined,
				accountFetchedOrdinal: 0n,
				accountRequestOrdinal: 0n,
				lastFewWitnessesPerUser: new Map(),
				adminHistory: [],
			}
			this.runInfoByAddress.set(runAddress, runInfo)
		}
		return runInfo
	}

	public saveRunState(runAddress: Pubkey, runState: CoordinatorDataRunState) {
		// TODO - handle fetch failure due to account being closed
		let runInfo = this.getRunInfo(runAddress)
		runInfo.accountState = runState
		runInfo.accountUpdatedAt = new Date()
		runInfo.accountFetchedOrdinal = runInfo.accountRequestOrdinal
		this.runAddressByRunId.set(runState.runId, runAddress)
	}
}

export const coordinatorDataStoreJsonCodec: JsonCodec<CoordinatorDataStore> =
	jsonCodecTransform(
		jsonCodecObject({
			runAddressByRunId: jsonCodecObjectToMap(
				{
					keyEncoder: (key: string) => key,
					keyDecoder: (key: string) => key,
				},
				jsonCodecPubkey
			),
			runInfoByAddress: utilsObjectToPubkeyMapJsonCodec(
				coordinatorDataRunInfoJsonCodec
			),
		}),
		{
			decoder: (encoded) =>
				new CoordinatorDataStore(
					encoded.runAddressByRunId,
					encoded.runInfoByAddress
				),
			encoder: (decoded) => ({
				runAddressByRunId: decoded.runAddressByRunId,
				runInfoByAddress: decoded.runInfoByAddress,
			}),
		}
	)
