import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecArrayToObject,
	jsonCodecBoolean,
	jsonCodecDateTime,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecObject,
	jsonCodecOptional,
	jsonCodecPubkey,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
	Pubkey,
} from 'solana-kiss'
import {
	utilsObjectToPubkeyMapJsonCodec,
	utilsObjectToStringMapJsonCodec,
} from '../utils'
import {
	CoordinatorDataRunState,
	coordinatorDataRunStateJsonCodec,
} from './CoordinatorDataRunState'

export interface CoordinatorDataRunInfoWitness {
	blockTime: Date | undefined
	ordinal: bigint
	proof: {
		position: bigint
		index: bigint
		witness: boolean
	}
	metadata: {
		tokensPerSec: number
		bandwidthPerSec: number
		loss: number
		step: number
	}
}

export interface CoordinatorDataRunInfo {
	accountState: CoordinatorDataRunState | undefined
	accountUpdatedAt: Date | undefined
	accountFetchedOrdinal: bigint
	accountRequestOrdinal: bigint
	witnessesPerUser: Map<
		Pubkey,
		{
			lastFew: Array<CoordinatorDataRunInfoWitness>
			sampled: {
				rate: number
				data: Array<{
					selector: number
					witness: CoordinatorDataRunInfoWitness
				}>
			}
		}
	>
	adminHistory: Array<{
		blockTime: Date | undefined
		instructionOrdinal: bigint
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
	}>
}

const witnessJsonCodec: JsonCodec<CoordinatorDataRunInfoWitness> =
	jsonCodecObject({
		blockTime: jsonCodecOptional(jsonCodecDateTime),
		ordinal: jsonCodecInteger,
		proof: jsonCodecObject({
			position: jsonCodecInteger,
			index: jsonCodecInteger,
			witness: jsonCodecBoolean,
		}),
		metadata: jsonCodecObject({
			tokensPerSec: jsonCodecNumber,
			bandwidthPerSec: jsonCodecNumber,
			loss: jsonCodecNumber,
			step: jsonCodecNumber,
		}),
	})

export const coordinatorDataRunInfoJsonCodec: JsonCodec<CoordinatorDataRunInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(coordinatorDataRunStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		accountFetchedOrdinal: jsonCodecInteger,
		accountRequestOrdinal: jsonCodecInteger,
		witnessesPerUser: utilsObjectToPubkeyMapJsonCodec(
			jsonCodecObject({
				lastFew: jsonCodecArray(witnessJsonCodec),
				sampled: jsonCodecObject({
					rate: jsonCodecNumber,
					data: jsonCodecArray(
						jsonCodecArrayToObject({
							selector: jsonCodecNumber,
							witness: witnessJsonCodec,
						})
					),
				}),
			})
		),
		adminHistory: jsonCodecArray(
			jsonCodecObject({
				blockTime: jsonCodecOptional(jsonCodecDateTime),
				instructionOrdinal: jsonCodecInteger,
				instructionName: jsonCodecString,
				instructionAddresses: utilsObjectToStringMapJsonCodec(jsonCodecPubkey),
				instructionPayload: jsonCodecRaw,
			})
		),
	})
