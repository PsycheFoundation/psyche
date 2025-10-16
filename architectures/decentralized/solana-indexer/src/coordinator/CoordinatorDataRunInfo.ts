import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecDateTime,
	jsonCodecInteger,
	jsonCodecObject,
	jsonCodecOptional,
	jsonCodecPubkey,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
	Pubkey,
} from 'solana-kiss'
import { utilsObjectToStringMapJsonCodec } from '../utils'
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
		step: number
		stats: Map<string, number>
	}
}

export interface CoordinatorDataRunInfo {
	accountState: CoordinatorDataRunState | undefined
	accountUpdatedAt: Date | undefined
	accountFetchedOrdinal: bigint
	accountRequestOrdinal: bigint
	adminHistory: Array<{
		blockTime: Date | undefined
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
		instructionOrdinal: bigint
	}>
}

/*
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
		*/

export const coordinatorDataRunInfoJsonCodec: JsonCodec<CoordinatorDataRunInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(coordinatorDataRunStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		accountFetchedOrdinal: jsonCodecInteger,
		accountRequestOrdinal: jsonCodecInteger,
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
