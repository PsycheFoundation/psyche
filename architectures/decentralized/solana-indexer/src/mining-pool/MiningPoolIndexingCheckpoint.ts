import {
	IdlProgram,
	jsonCodecBoolean,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecString,
	jsonDecoderObjectEncodedSnakeKeys,
	RpcHttp,
} from 'solana-kiss'
import { utilsGetAndDecodeAccountState } from '../utils'
import { MiningPoolDataStore } from './MiningPoolDataStore'

export async function miningPoolIndexingCheckpoint(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	dataStore: MiningPoolDataStore
) {
	for (const [poolAddress, poolInfo] of dataStore.poolInfoByAddress) {
		if (poolInfo.accountFetchedOrdering === poolInfo.accountRequestOrdering) {
			break
		}
		try {
			const poolState = await utilsGetAndDecodeAccountState(
				rpcHttp,
				programIdl,
				poolAddress,
				poolStateJsonDecoder
			)
			dataStore.savePoolState(poolAddress, poolState)
		} catch (error) {
			console.error('Failed to refresh pool account state', poolAddress, error)
		}
	}
}

const poolStateJsonDecoder = jsonDecoderObjectEncodedSnakeKeys({
	bump: jsonCodecNumber.decoder,
	index: jsonCodecInteger.decoder,
	authority: jsonCodecString.decoder,
	collateralMint: jsonCodecString.decoder,
	maxDepositCollateralAmount: jsonCodecInteger.decoder,
	totalDepositedCollateralAmount: jsonCodecInteger.decoder,
	totalExtractedCollateralAmount: jsonCodecInteger.decoder,
	claimingEnabled: jsonCodecBoolean.decoder,
	redeemableMint: jsonCodecString.decoder,
	totalClaimedRedeemableAmount: jsonCodecInteger.decoder,
	freeze: jsonCodecBoolean.decoder,
})
