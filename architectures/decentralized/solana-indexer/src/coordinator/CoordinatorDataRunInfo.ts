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
import {
	utilsObjectToPubkeyMapJsonCodec,
	utilsObjectToStringMapJsonCodec,
} from '../utils'
import {
	CoordinatorDataRunState,
	coordinatorDataRunStateJsonCodec,
} from './CoordinatorDataRunState'

export interface CoordinatorDataRunInfoSample {
	minTime: Date | undefined
	maxTime: Date | undefined
	minOrdinal: bigint
	maxOrdinal: bigint
	minStep: number
	maxStep: number
	minValue: number
	maxValue: number
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
	adminHistory: Array<{
		blockTime: Date | undefined
		instructionOrdinal: bigint
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
	}>
}

const coordinatorDataRunInfoSampleJsonCodec: JsonCodec<CoordinatorDataRunInfoSample> =
	jsonCodecObject({
		minTime: jsonCodecOptional(jsonCodecDateTime),
		maxTime: jsonCodecOptional(jsonCodecDateTime),
		minOrdinal: jsonCodecInteger,
		maxOrdinal: jsonCodecInteger,
		minStep: jsonCodecNumber,
		maxStep: jsonCodecNumber,
		minValue: jsonCodecNumber,
		maxValue: jsonCodecNumber,
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
