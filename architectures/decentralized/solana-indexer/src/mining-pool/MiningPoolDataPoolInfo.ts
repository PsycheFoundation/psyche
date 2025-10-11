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
import {
	utilsObjectToPubkeyMapJsonCodec,
	utilsObjectToStringMapJsonCodec,
} from '../utils'
import {
	MiningPoolDataPoolState,
	miningPoolDataPoolStateJsonCodec,
} from './MiningPoolDataPoolState'

export interface MiningPoolDataPoolInfo {
	accountState: MiningPoolDataPoolState | undefined
	accountUpdatedAt: Date | undefined
	accountFetchedOrdering: bigint
	accountRequestOrdering: bigint
	totalExtractCollateralAmount: bigint
	depositCollateralAmountPerUser: Map<Pubkey, bigint>
	totalDepositCollateralAmount: bigint
	claimRedeemableAmountPerUser: Map<Pubkey, bigint>
	totalClaimRedeemableAmount: bigint
	adminHistory: Array<{
		processedTime: Date | undefined
		signerAddress: Pubkey
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
		ordering: bigint
	}>
}

export const miningPoolDataPoolInfoJsonCodec: JsonCodec<MiningPoolDataPoolInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(miningPoolDataPoolStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		accountFetchedOrdering: jsonCodecInteger,
		accountRequestOrdering: jsonCodecInteger,
		totalExtractCollateralAmount: jsonCodecInteger,
		depositCollateralAmountPerUser:
			utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
		totalDepositCollateralAmount: jsonCodecInteger,
		claimRedeemableAmountPerUser:
			utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
		totalClaimRedeemableAmount: jsonCodecInteger,
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
