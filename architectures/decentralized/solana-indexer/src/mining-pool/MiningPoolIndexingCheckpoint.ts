import {
	casingCamelToSnake,
	IdlProgram,
	jsonDecoderObject,
	jsonTypeBoolean,
	jsonTypeInteger,
	jsonTypeNumber,
	jsonTypeString,
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

const poolStateJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
	bump: jsonTypeNumber.decoder,
	index: jsonTypeInteger.decoder,
	authority: jsonTypeString.decoder,
	collateralMint: jsonTypeString.decoder,
	maxDepositCollateralAmount: jsonTypeInteger.decoder,
	totalDepositedCollateralAmount: jsonTypeInteger.decoder,
	totalExtractedCollateralAmount: jsonTypeInteger.decoder,
	claimingEnabled: jsonTypeBoolean.decoder,
	redeemableMint: jsonTypeString.decoder,
	totalClaimedRedeemableAmount: jsonTypeInteger.decoder,
	freeze: jsonTypeBoolean.decoder,
})
