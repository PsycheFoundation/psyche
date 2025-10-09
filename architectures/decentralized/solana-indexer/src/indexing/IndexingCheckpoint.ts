import {
	JsonType,
	jsonTypeArray,
	jsonTypeInteger,
	jsonTypeNumber,
	jsonTypeObject,
	jsonTypeSignature,
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

export const indexingCheckpointJsonType: JsonType<IndexingCheckpoint> =
	jsonTypeObject((key) => key, {
		indexedChunks: jsonTypeArray(
			jsonTypeObject((key) => key, {
				orderingHigh: jsonTypeInteger,
				orderingLow: jsonTypeInteger,
				startedFrom: jsonTypeSignature,
				rewindedUntil: jsonTypeSignature,
				processedCounter: jsonTypeNumber,
			})
		),
	})
