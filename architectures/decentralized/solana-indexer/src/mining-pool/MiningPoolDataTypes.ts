import {
	jsonCodecArrayToArray,
	jsonCodecBigInt,
	jsonCodecBoolean,
	JsonCodecContent,
	jsonCodecDateTime,
	jsonCodecNullable,
	jsonCodecNumber,
	jsonCodecObjectToObject,
	jsonCodecPubkey,
	jsonCodecValue,
} from 'solana-kiss'
import { indexerInstructionJsonCodec } from '../indexer/IndexerTypes'
import { jsonCodecObjectToMapByPubkey } from '../json'

export type MiningPoolDataPoolOnchain = JsonCodecContent<
	typeof miningPoolDataPoolOnchainJsonCodec
>

export type MiningPoolDataPoolAnalysis = JsonCodecContent<
	typeof miningPoolDataPoolAnalysisJsonCodec
>

export const miningPoolDataPoolOnchainJsonCodec = jsonCodecObjectToObject({
	bump: jsonCodecNumber,
	index: jsonCodecBigInt,
	authority: jsonCodecPubkey,
	collateralMint: jsonCodecPubkey,
	maxDepositCollateralAmount: jsonCodecBigInt,
	totalDepositedCollateralAmount: jsonCodecBigInt,
	totalExtractedCollateralAmount: jsonCodecBigInt,
	claimingEnabled: jsonCodecBoolean,
	redeemableMint: jsonCodecPubkey,
	totalClaimedRedeemableAmount: jsonCodecBigInt,
	freeze: jsonCodecBoolean,
})

export const miningPoolDataPoolAnalysisJsonCodec = jsonCodecObjectToObject({
	latestKnownChangeOrdinal: jsonCodecBigInt,
	latestUpdateFetchOrdinal: jsonCodecBigInt,
	latestOnchainSnapshot: jsonCodecNullable(
		jsonCodecObjectToObject({
			parsed: miningPoolDataPoolOnchainJsonCodec,
			native: jsonCodecValue,
			updatedAt: jsonCodecDateTime,
		})
	),
	depositCollateralAmountPerUser: jsonCodecObjectToMapByPubkey(jsonCodecBigInt),
	claimRedeemableAmountPerUser: jsonCodecObjectToMapByPubkey(jsonCodecBigInt),
	totalDepositCollateralAmount: jsonCodecBigInt,
	totalClaimRedeemableAmount: jsonCodecBigInt,
	totalExtractCollateralAmount: jsonCodecBigInt,
	adminHistory: jsonCodecArrayToArray(indexerInstructionJsonCodec),
})
