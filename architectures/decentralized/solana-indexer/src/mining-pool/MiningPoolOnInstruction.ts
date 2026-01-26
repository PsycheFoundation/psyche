import { jsonCodecBigInt, jsonDecoderObjectToObject, Pubkey } from 'solana-kiss'
import { IndexerInstruction } from '../indexer/IndexerTypes'
import { utilsBigintArraySortAscending } from '../utils'
import { MiningPoolDataStore } from './MiningPoolDataStore'
import { MiningPoolDataPoolAnalysis } from './MiningPoolDataTypes'

export function miningPoolOnInstruction(
	dataStore: MiningPoolDataStore,
	instruction: IndexerInstruction
) {
	const poolAddress = instruction.instructionAddresses['pool']
	if (poolAddress === undefined) {
		throw new Error(
			'MiningPool: Instruction: PoolExtract: Missing pool address'
		)
	}
	const signerAddress =
		instruction.instructionAddresses['authority'] ??
		instruction.instructionAddresses['user']
	if (signerAddress === undefined) {
		throw new Error('MiningPool: Instruction: Could not find signer address')
	}
	const poolAnalysis = dataStore.getPoolAnalysis(poolAddress)
	const processors = processorsByInstructionName.get(
		instruction.instructionName
	)
	if (processors !== undefined) {
		for (const processor of processors) {
			processor(poolAnalysis, {
				poolAddress,
				signerAddress,
				instruction: instruction,
			})
		}
	} else {
		console.warn(
			'MiningPool: Unknown instruction:',
			instruction.instructionName
		)
	}
	if (instruction.instructionOrdinal > poolAnalysis.latestKnownChangeOrdinal) {
		poolAnalysis.latestKnownChangeOrdinal = instruction.instructionOrdinal
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
	instruction: IndexerInstruction
}

async function processAdminAction(
	poolAnalysis: MiningPoolDataPoolAnalysis,
	context: ProcessingContext
): Promise<void> {
	poolAnalysis.adminHistory.push(context.instruction)
	utilsBigintArraySortAscending(
		poolAnalysis.adminHistory,
		(adminAction) => adminAction.instructionOrdinal
	)
}

async function processPoolExtract(
	poolAnalysis: MiningPoolDataPoolAnalysis,
	context: ProcessingContext
): Promise<void> {
	const poolExtractPayload = poolExtractJsonDecoder(
		context.instruction.instructionPayload
	)
	poolAnalysis.totalExtractCollateralAmount +=
		poolExtractPayload.params.collateralAmount
}

async function processLenderDeposit(
	poolAnalysis: MiningPoolDataPoolAnalysis,
	context: ProcessingContext
): Promise<void> {
	const lenderDepositPayload = lenderDepositJsonDecoder(
		context.instruction.instructionPayload
	)
	poolAnalysis.depositCollateralAmountPerUser.set(
		context.signerAddress,
		(poolAnalysis.depositCollateralAmountPerUser.get(context.signerAddress) ??
			0n) + lenderDepositPayload.params.collateralAmount
	)
	poolAnalysis.totalDepositCollateralAmount +=
		lenderDepositPayload.params.collateralAmount
}

async function processLenderClaim(
	poolAnalysis: MiningPoolDataPoolAnalysis,
	context: ProcessingContext
): Promise<void> {
	const lenderClaimPayload = lenderClaimJsonDecoder(
		context.instruction.instructionPayload
	)
	poolAnalysis.claimRedeemableAmountPerUser.set(
		context.signerAddress,
		(poolAnalysis.claimRedeemableAmountPerUser.get(context.signerAddress) ??
			0n) + lenderClaimPayload.params.redeemableAmount
	)
	poolAnalysis.totalClaimRedeemableAmount +=
		lenderClaimPayload.params.redeemableAmount
}

const poolExtractJsonDecoder = jsonDecoderObjectToObject({
	params: jsonDecoderObjectToObject({
		collateralAmount: jsonCodecBigInt.decoder,
	}),
})

const lenderDepositJsonDecoder = jsonDecoderObjectToObject({
	params: jsonDecoderObjectToObject({
		collateralAmount: jsonCodecBigInt.decoder,
	}),
})

const lenderClaimJsonDecoder = jsonDecoderObjectToObject({
	params: jsonDecoderObjectToObject({
		redeemableAmount: jsonCodecBigInt.decoder,
	}),
})
