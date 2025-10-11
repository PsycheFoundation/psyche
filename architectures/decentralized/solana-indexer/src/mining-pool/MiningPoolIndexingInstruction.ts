import {
	JsonValue,
	Pubkey,
	jsonCodecInteger,
	jsonDecoderObjectWithKeysSnakeEncoded,
} from 'solana-kiss'
import { MiningPoolDataStore } from './MiningPoolDataStore'

export async function miningPoolIndexingInstruction(
	dataStore: MiningPoolDataStore,
	blockTime: Date | undefined,
	instructionName: string,
	instructionAddresses: Map<string, Pubkey>,
	instructionPayload: JsonValue,
	instructionOrdinal: bigint
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
				instructionName,
				instructionAddresses,
				instructionPayload,
				instructionOrdinal,
				blockTime,
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
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
	instructionOrdinal: bigint
	blockTime: Date | undefined
}

async function processAdminAction(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.adminHistory.push({
		blockTime: context.blockTime,
		instructionName: context.instructionName,
		instructionAddresses: context.instructionAddresses,
		instructionPayload: context.instructionPayload,
		instructionOrdinal: context.instructionOrdinal,
	})
	poolInfo.adminHistory.sort((a, b) =>
		Number(b.instructionOrdinal - a.instructionOrdinal)
	)
}

async function processPoolExtract(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const instructionParams = poolExtractArgsJsonDecoder(
		context.instructionPayload
	).params
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	poolInfo.totalExtractCollateralAmount += instructionParams.collateralAmount
}

async function processLenderDeposit(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const instructionParams = lenderDepositArgsJsonDecoder(
		context.instructionPayload
	).params
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	const depositAmountBefore =
		poolInfo.depositCollateralAmountPerUser.get(context.signerAddress) ?? 0n
	const depositAmountAfter =
		depositAmountBefore + instructionParams.collateralAmount
	poolInfo.depositCollateralAmountPerUser.set(
		context.signerAddress,
		depositAmountAfter
	)
	poolInfo.totalDepositCollateralAmount += instructionParams.collateralAmount
}

async function processLenderClaim(
	dataStore: MiningPoolDataStore,
	context: ProcessingContext
): Promise<void> {
	const instructionParams = lenderClaimArgsJsonDecoder(
		context.instructionPayload
	).params
	let poolInfo = dataStore.getPoolInfo(context.poolAddress)
	const redeemableAmountBefore =
		poolInfo.claimRedeemableAmountPerUser.get(context.signerAddress) ?? 0n
	const redeemableAmountAfter =
		redeemableAmountBefore + instructionParams.redeemableAmount
	poolInfo.claimRedeemableAmountPerUser.set(
		context.signerAddress,
		redeemableAmountAfter
	)
	poolInfo.totalClaimRedeemableAmount += instructionParams.redeemableAmount
}

const poolExtractArgsJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		collateralAmount: jsonCodecInteger.decoder,
	}),
})

const lenderDepositArgsJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		collateralAmount: jsonCodecInteger.decoder,
	}),
})

const lenderClaimArgsJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	params: jsonDecoderObjectWithKeysSnakeEncoded({
		redeemableAmount: jsonCodecInteger.decoder,
	}),
})
