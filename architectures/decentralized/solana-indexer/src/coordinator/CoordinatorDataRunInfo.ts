import {
	JsonCodec,
	jsonCodecArray,
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
import { utilsObjectToStringMapJsonCodec } from '../utils'
import {
	CoordinatorDataRunState,
	coordinatorDataRunStateJsonCodec,
} from './CoordinatorDataRunState'

export interface CoordinatorDataRunInfoWitness {
	blockTime: Date | undefined
	ordinal: bigint
	position: bigint
	index: bigint
	step: number
	stats: Map<string, number>
}

export interface CoordinatorDataRunInfo {
	accountState: CoordinatorDataRunState | undefined
	accountUpdatedAt: Date | undefined
	accountFetchedOrdinal: bigint
	accountRequestOrdinal: bigint
	witnessHistory: Array<CoordinatorDataRunInfoWitness>
	slicesOrdinals: Array<bigint>
	adminHistory: Array<{
		blockTime: Date | undefined
		instructionOrdinal: bigint
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
	}>
}

const coordinatorDataRunInfoWitnessJsonCodec: JsonCodec<CoordinatorDataRunInfoWitness> =
	jsonCodecObject({
		blockTime: jsonCodecOptional(jsonCodecDateTime),
		ordinal: jsonCodecInteger,
		position: jsonCodecInteger,
		index: jsonCodecInteger,
		step: jsonCodecNumber,
		stats: utilsObjectToStringMapJsonCodec(jsonCodecNumber),
	})

export const coordinatorDataRunInfoJsonCodec: JsonCodec<CoordinatorDataRunInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(coordinatorDataRunStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		accountFetchedOrdinal: jsonCodecInteger,
		accountRequestOrdinal: jsonCodecInteger,
		witnessHistory: jsonCodecArray(coordinatorDataRunInfoWitnessJsonCodec),
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
