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
	accountFetchedOrdinal: bigint
	accountRequestOrdinal: bigint
	totalExtractCollateralAmount: bigint
	depositCollateralAmountPerUser: Map<Pubkey, bigint>
	totalDepositCollateralAmount: bigint
	claimRedeemableAmountPerUser: Map<Pubkey, bigint>
	totalClaimRedeemableAmount: bigint
	adminHistory: Array<{
		blockTime: Date | undefined
		instructionOrdinal: bigint
		instructionName: string
		instructionAddresses: Map<string, Pubkey>
		instructionPayload: JsonValue
	}>
}

export const miningPoolDataPoolInfoJsonCodec: JsonCodec<MiningPoolDataPoolInfo> =
	jsonCodecObject({
		accountState: jsonCodecOptional(miningPoolDataPoolStateJsonCodec),
		accountUpdatedAt: jsonCodecOptional(jsonCodecDateTime),
		accountFetchedOrdinal: jsonCodecInteger,
		accountRequestOrdinal: jsonCodecInteger,
		totalExtractCollateralAmount: jsonCodecInteger,
		depositCollateralAmountPerUser:
			utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
		totalDepositCollateralAmount: jsonCodecInteger,
		claimRedeemableAmountPerUser:
			utilsObjectToPubkeyMapJsonCodec(jsonCodecInteger),
		totalClaimRedeemableAmount: jsonCodecInteger,
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
