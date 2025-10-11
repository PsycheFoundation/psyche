import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecObject,
	jsonCodecSignature,
	Signature,
} from 'solana-kiss'

export type IndexingCheckpointChunk = {
	orderingHigh: bigint
	orderingLow: bigint
	startedFrom: Signature
	rewindedUntil: Signature
	processedCounter: number
}

export type IndexingCheckpoint = {
	indexedChunks: Array<IndexingCheckpointChunk>
}

export const indexingCheckpointJsonCodec: JsonCodec<IndexingCheckpoint> =
	jsonCodecObject({
		indexedChunks: jsonCodecArray(
			jsonCodecObject({
				orderingHigh: jsonCodecInteger,
				orderingLow: jsonCodecInteger,
				startedFrom: jsonCodecSignature,
				rewindedUntil: jsonCodecSignature,
				processedCounter: jsonCodecNumber,
			})
		),
	})
