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
