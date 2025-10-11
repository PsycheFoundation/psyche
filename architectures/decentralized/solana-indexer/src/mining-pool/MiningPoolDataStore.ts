import {
	Pubkey,
	jsonCodecObject,
	jsonCodecObjectToMap,
	jsonCodecPubkey,
	jsonCodecTransform,
} from 'solana-kiss'
import { utilsObjectToPubkeyMapJsonCodec } from '../utils'
import {
	MiningPoolDataPoolInfo,
	miningPoolDataPoolInfoJsonCodec,
} from './MiningPoolDataPoolInfo'
import { MiningPoolDataPoolState } from './MiningPoolDataPoolState'

export class MiningPoolDataStore {
	poolAddressByIndex: Map<bigint, Pubkey>
	poolInfoByAddress: Map<Pubkey, MiningPoolDataPoolInfo>

	constructor(
		poolAddressByIndex: Map<bigint, Pubkey>,
		poolInfoByAddress: Map<Pubkey, MiningPoolDataPoolInfo>
	) {
		this.poolAddressByIndex = poolAddressByIndex
		this.poolInfoByAddress = poolInfoByAddress
	}

	public getPoolInfo(poolAddress: Pubkey): MiningPoolDataPoolInfo {
		let poolInfo = this.poolInfoByAddress.get(poolAddress)
		if (poolInfo === undefined) {
			poolInfo = {
				accountState: undefined,
				accountUpdatedAt: undefined,
				accountFetchedOrdinal: 0n,
				accountRequestOrdinal: 0n,
				totalExtractCollateralAmount: 0n,
				depositCollateralAmountPerUser: new Map<Pubkey, bigint>(),
				totalDepositCollateralAmount: 0n,
				claimRedeemableAmountPerUser: new Map<Pubkey, bigint>(),
				totalClaimRedeemableAmount: 0n,
				adminHistory: [],
			}
			this.poolInfoByAddress.set(poolAddress, poolInfo)
		}
		return poolInfo
	}

	public savePoolState(
		poolAddress: Pubkey,
		poolState: MiningPoolDataPoolState
	) {
		let poolInfo = this.getPoolInfo(poolAddress)
		poolInfo.accountState = poolState
		poolInfo.accountUpdatedAt = new Date()
		poolInfo.accountFetchedOrdinal = poolInfo.accountRequestOrdinal
		this.poolAddressByIndex.set(poolState.index, poolAddress)
	}
}

export const miningPoolDataStoreJsonCodec = jsonCodecTransform(
	jsonCodecObject({
		poolAddressByIndex: jsonCodecObjectToMap(
			{
				keyEncoder: (key: bigint) => String(key),
				keyDecoder: (key: string) => BigInt(key),
			},
			jsonCodecPubkey
		),
		poolInfoByAddress: utilsObjectToPubkeyMapJsonCodec(
			miningPoolDataPoolInfoJsonCodec
		),
	}),
	{
		decoder: (encoded) =>
			new MiningPoolDataStore(
				encoded.poolAddressByIndex,
				encoded.poolInfoByAddress
			),
		encoder: (decoded) => ({
			poolAddressByIndex: decoded.poolAddressByIndex,
			poolInfoByAddress: decoded.poolInfoByAddress,
		}),
	}
)
