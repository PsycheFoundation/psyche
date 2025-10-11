import {
	JsonValue,
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
	public poolAddressByIndex: Map<bigint, Pubkey>
	public poolInfoByAddress: Map<Pubkey, MiningPoolDataPoolInfo>

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
				accountFetchedOrdering: 0n,
				accountRequestOrdering: 0n,
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
		poolInfo.accountFetchedOrdering = poolInfo.accountRequestOrdering
		this.poolAddressByIndex.set(poolState.index, poolAddress)
	}

	public savePoolExtract(poolAddress: Pubkey, collateralAmount: bigint) {
		let poolInfo = this.getPoolInfo(poolAddress)
		poolInfo.totalExtractCollateralAmount += collateralAmount
	}

	public savePoolDeposit(
		poolAddress: Pubkey,
		signerAddress: Pubkey,
		depositAmount: bigint
	) {
		let poolInfo = this.getPoolInfo(poolAddress)
		const depositAmountBefore =
			poolInfo.depositCollateralAmountPerUser.get(signerAddress) ?? 0n
		const depositAmountAfter = depositAmountBefore + depositAmount
		poolInfo.depositCollateralAmountPerUser.set(
			signerAddress,
			depositAmountAfter
		)
		poolInfo.totalDepositCollateralAmount += depositAmount
	}

	public savePoolClaim(
		poolAddress: Pubkey,
		signerAddress: Pubkey,
		redeemableAmount: bigint
	) {
		let poolInfo = this.getPoolInfo(poolAddress)
		const redeemableAmountBefore =
			poolInfo.claimRedeemableAmountPerUser.get(signerAddress) ?? 0n
		const redeemableAmountAfter = redeemableAmountBefore + redeemableAmount
		poolInfo.claimRedeemableAmountPerUser.set(
			signerAddress,
			redeemableAmountAfter
		)
		poolInfo.totalClaimRedeemableAmount += redeemableAmount
	}

	public savePoolAdminAction(
		poolAddress: Pubkey,
		signerAddress: Pubkey,
		instructionName: string,
		instructionAddresses: Map<string, Pubkey>,
		instructionPayload: JsonValue,
		ordering: bigint,
		processedTime: Date | undefined
	) {
		let poolInfo = this.getPoolInfo(poolAddress)
		poolInfo.adminHistory.push({
			processedTime,
			signerAddress,
			instructionName,
			instructionAddresses,
			instructionPayload,
			ordering,
		})
		poolInfo.adminHistory.sort((a, b) => Number(b.ordering - a.ordering))
	}

	public setPoolRequestOrdering(poolAddress: Pubkey, ordering: bigint) {
		const poolInfo = this.getPoolInfo(poolAddress)
		if (ordering > poolInfo.accountRequestOrdering) {
			poolInfo.accountRequestOrdering = ordering
		}
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
