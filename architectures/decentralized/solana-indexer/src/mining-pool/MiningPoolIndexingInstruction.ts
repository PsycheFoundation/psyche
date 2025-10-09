import {
	JsonValue,
	Pubkey,
	casingCamelToSnake,
	jsonDecoderObject,
	jsonTypeInteger,
} from 'solana-kiss'
import { MiningPoolDataStore } from './MiningPoolDataStore'

export async function miningPoolIndexingInstruction(
	dataStore: MiningPoolDataStore,
	instructionName: string,
	instructionAddresses: Map<string, Pubkey>,
	instructionPayload: JsonValue,
	ordering: bigint,
	processedTime: Date | undefined
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
				ordering,
				processedTime,
			})
		}
	} else {
		console.warn('MiningPool: Unknown instruction:', instructionName)
	}
	dataStore.setPoolRequestOrdering(poolAddress, ordering)
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

type ProcessingContent = {
	poolAddress: Pubkey
	signerAddress: Pubkey
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
	ordering: bigint
	processedTime: Date | undefined
}

async function processAdminAction(
	dataStore: MiningPoolDataStore,
	content: ProcessingContent
): Promise<void> {
	dataStore.savePoolAdminAction(
		content.poolAddress,
		content.signerAddress,
		content.instructionName,
		content.instructionAddresses,
		content.instructionPayload,
		content.ordering,
		content.processedTime
	)
}

async function processPoolExtract(
	dataStore: MiningPoolDataStore,
	content: ProcessingContent
): Promise<void> {
	const instructionParams = poolExtractArgsJsonDecoder(
		content.instructionPayload
	).params
	dataStore.savePoolExtract(
		content.poolAddress,
		instructionParams.collateralAmount
	)
}

async function processLenderDeposit(
	dataStore: MiningPoolDataStore,
	content: ProcessingContent
): Promise<void> {
	const instructionParams = lenderDepositArgsJsonDecoder(
		content.instructionPayload
	).params
	dataStore.savePoolDeposit(
		content.poolAddress,
		content.signerAddress,
		instructionParams.collateralAmount
	)
}

async function processLenderClaim(
	dataStore: MiningPoolDataStore,
	content: ProcessingContent
): Promise<void> {
	const instructionParams = lenderClaimArgsJsonDecoder(
		content.instructionPayload
	).params
	dataStore.savePoolClaim(
		content.poolAddress,
		content.signerAddress,
		instructionParams.redeemableAmount
	)
}

const poolExtractArgsJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
	params: jsonDecoderObject(casingCamelToSnake, {
		collateralAmount: jsonTypeInteger.decoder,
	}),
})

const lenderDepositArgsJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
	params: jsonDecoderObject(casingCamelToSnake, {
		collateralAmount: jsonTypeInteger.decoder,
	}),
})

const lenderClaimArgsJsonDecoder = jsonDecoderObject(casingCamelToSnake, {
	params: jsonDecoderObject(casingCamelToSnake, {
		redeemableAmount: jsonTypeInteger.decoder,
	}),
})
