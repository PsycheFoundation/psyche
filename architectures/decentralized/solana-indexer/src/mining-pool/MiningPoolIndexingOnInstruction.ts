import {
	JsonValue,
	Pubkey,
	jsonCodecInteger,
	jsonDecoderObjectWithKeysSnakeEncoded,
} from 'solana-kiss'
import { MiningPoolDataStore } from './MiningPoolDataStore'

export async function miningPoolIndexingOnInstruction(
	dataStore: MiningPoolDataStore,
	blockTime: Date | undefined,
	instructionOrdinal: bigint,
	instructionName: string,
	instructionAddresses: Map<string, Pubkey>,
	instructionPayload: JsonValue
) {
	const poolAddress = instructionAddresses.get('pool')
	if (poolAddress === undefined) {
		throw new Error(
			'MiningPool: Instruction: PoolExtract: Missing pool address'
		)
	}
	const signerAddress =
		instructionAddresses.get('authority') ?? instructionAddresses.get('user')
	if (signerAddress === undefined) {
		throw new Error('MiningPool: Instruction: Could not find signer address')
	}
	const processors = processorsByInstructionName.get(instructionName)
	if (processors !== undefined) {
		for (const processor of processors) {
			await processor(dataStore, {
				poolAddress,
				signerAddress,
				blockTime,
				instructionOrdinal,
				instructionName,
				instructionAddresses,
				instructionPayload,
			})
		}
	} else {
		console.warn('MiningPool: Unknown instruction:', instructionName)
	}
	const poolInfo = dataStore.getPoolInfo(poolAddress)
	if (instructionOrdinal > poolInfo.accountRequestOrdinal) {
		poolInfo.accountRequestOrdinal = instructionOrdinal
	}
}

const processorsByInstructionName = new Map([
	['pool_create', [processAdminAction]],
	['pool_update', [processAdminAction]],
	['pool_extract', [processAdminAction, processPoolExtract]],
	['pool_claimable', [processAdminAction]],
	['lender_create', []],
	['lender_deposit', [processLenderDeposit]],
	['lender_claim', [processLenderClaim]],
])

type ProcessingContext = {
	poolAddress: Pubkey
	signerAddress: Pubkey
	blockTime: Date | undefined
	instructionOrdinal: bigint
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
}

async function processAdminAction(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.adminHistory.push({
		blockTime: context.blockTime,
		instructionOrdinal: context.instructionOrdinal,
		instructionName: context.instructionName,
		instructionAddresses: context.instructionAddresses,
		instructionPayload: context.instructionPayload,
	})
	poolInfo.adminHistory.sort((a, b) =>
		Number(b.instructionOrdinal - a.instructionOrdinal)
	)
}

async function processPoolExtract(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const poolExtractPayload = poolExtractJsonDecoder(context.instructionPayload)
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.totalExtractCollateralAmount +=
		poolExtractPayload.params.collateralAmount
}

async function processLenderDeposit(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const lenderDepositPayload = lenderDepositJsonDecoder(
		context.instructionPayload
	)
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.depositCollateralAmountPerUser.set(
		context.signerAddress,
		(poolInfo.depositCollateralAmountPerUser.get(context.signerAddress) ?? 0n) +
			lenderDepositPayload.params.collateralAmount
	)
	poolInfo.totalDepositCollateralAmount +=
		lenderDepositPayload.params.collateralAmount
}

async function processLenderClaim(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const lenderClaimPayload = lenderClaimJsonDecoder(context.instructionPayload)
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.claimRedeemableAmountPerUser.set(
		context.signerAddress,
		(poolInfo.claimRedeemableAmountPerUser.get(context.signerAddress) ?? 0n) +
			lenderClaimPayload.params.redeemableAmount
	)
	poolInfo.totalClaimRedeemableAmount +=
		lenderClaimPayload.params.redeemableAmount
}

const poolExtractJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		collateralAmount: jsonCodecInteger.decoder,
	}),
})

const lenderDepositJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		collateralAmount: jsonCodecInteger.decoder,
	}),
})

const lenderClaimJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		redeemableAmount: jsonCodecInteger.decoder,
	}),
})
