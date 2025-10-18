import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecArrayToObject,
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

export interface CoordinatorDataRunInfoSample {
	maxTime: Date | undefined
	maxOrdinal: bigint
	step: number
	sumValue: number
	numValue: number
}

export interface CoordinatorDataRunInfo {
	accountState: CoordinatorDataRunState | undefined
	accountUpdatedAt: Date | undefined
	changeAcknowledgedOrdinal: bigint
	changeNotificationOrdinal: bigint
	lastWitnessByUser: Map<Pubkey, { ordinal: bigint; step: number }>
	samplesByStatName: Map<string, Array<CoordinatorDataRunInfoSample>>
	finishesOrdinals: Array<bigint>
	importantHistory: Array<{
		blockTime: Date | undefined
		instructionOrdinal: bigint
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
	}>
}

const coordinatorDataRunInfoSampleJsonCodec: JsonCodec<CoordinatorDataRunInfoSample> =
	jsonCodecArrayToObject({
		maxTime: jsonCodecOptional(jsonCodecDateTime),
		maxOrdinal: jsonCodecInteger,
		step: jsonCodecNumber,
		sumValue: jsonCodecNumber,
		numValue: jsonCodecNumber,
	})

export const coordinatorDataRunInfoJsonCodec: JsonCodec<CoordinatorDataRunInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(coordinatorDataRunStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		changeAcknowledgedOrdinal: jsonCodecInteger,
		changeNotificationOrdinal: jsonCodecInteger,
		lastWitnessByUser: utilsObjectToPubkeyMapJsonCodec(
			jsonCodecObject({ ordinal: jsonCodecInteger, step: jsonCodecNumber })
		),
		samplesByStatName: utilsObjectToStringMapJsonCodec(
			jsonCodecArray(coordinatorDataRunInfoSampleJsonCodec)
		),
		finishesOrdinals: jsonCodecArray(jsonCodecInteger),
		importantHistory: jsonCodecArray(
			jsonCodecObject({
				blockTime: jsonCodecOptional(jsonCodecDateTime),
				instructionOrdinal: jsonCodecInteger,
				instructionName: jsonCodecString,
				instructionAddresses: utilsObjectToStringMapJsonCodec(jsonCodecPubkey),
				instructionPayload: jsonCodecRaw,
			})
		),
	})
