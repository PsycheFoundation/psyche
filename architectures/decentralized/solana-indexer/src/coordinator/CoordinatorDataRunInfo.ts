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
	processedTime: Date | undefined
	ordering: bigint
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
	accountFetchedOrdering: bigint
	accountRequestOrdering: bigint
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
		processedTime: Date | undefined
		signerAddress: Pubkey
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
		ordering: bigint
	}>
}

const witnessJsonCodec: JsonCodec<CoordinatorDataRunInfoWitness> =
	jsonCodecObject((key) => key, {
		processedTime: jsonCodecOptional(jsonCodecDateTime),
		ordering: jsonCodecInteger,
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
		accountFetchedOrdering: jsonCodecInteger,
		accountRequestOrdering: jsonCodecInteger,
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
				processedTime: jsonCodecOptional(jsonCodecDateTime),
				signerAddress: jsonCodecPubkey,
				instructionName: jsonCodecString,
				instructionAddresses: utilsObjectToStringMapJsonCodec(jsonCodecPubkey),
				instructionPayload: jsonCodecRaw,
				ordering: jsonCodecInteger,
			})
		),
	})
