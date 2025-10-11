import {
	JsonValue,
	Pubkey,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonDecoderObjectWithKeysSnakeEncoded,
} from 'solana-kiss'
import {
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
	utilsRustSmallBooleanJsonDecoder,
} from '../utils'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorIndexingInstruction(
	dataStore: CoordinatorDataStore,
	blockTime: Date | undefined,
	instructionOrdinal: bigint,
	instructionName: string,
	instructionAddresses: Map<string, Pubkey>,
	instructionPayload: JsonValue
): Promise<void> {
	const runAddress = instructionAddresses.get('coordinator_account')
	if (runAddress === undefined) {
		throw new Error('Coordinator: Instruction: Missing run address')
	}
	const signerAddress =
		instructionAddresses.get('payer') ??
		instructionAddresses.get('authority') ??
		instructionAddresses.get('user')
	if (signerAddress === undefined) {
		throw new Error('Coordinator: Instruction: Could not find signer address')
	}
	const processors = processorsByInstructionName.get(instructionName)
	if (processors !== undefined) {
		for (const processor of processors) {
			await processor(dataStore, {
				runAddress,
				signerAddress,
				blockTime,
				instructionOrdinal,
				instructionName,
				instructionAddresses,
				instructionPayload,
			})
		}
	} else {
		console.warn('Coordinator: Unknown instruction:', instructionName)
	}
	const runInfo = dataStore.getRunInfo(runAddress)
	if (instructionOrdinal > runInfo.accountRequestOrdinal) {
		runInfo.accountRequestOrdinal = instructionOrdinal
	}
}

const processorsByInstructionName = new Map([
	['init_coordinator', [processAdminAction]],
	['update', [processAdminAction]],
	['set_future_epoch_rates', [processAdminAction]],
	['set_paused', [processAdminAction]],
	['join_run', []], // TODO - how to handle join run?
	['warmup_witness', []], // TODO - how to handle warmup witness?
	['witness', [processWitness]],
	['tick', []],
	['checkpoint', []], // TODO - how to handle checkpoint?
	['health_check', []], // TODO - how to handle health check?
	['free_coordinator', [processAdminAction]],
])

type ProcessingContext = {
	runAddress: Pubkey
	signerAddress: Pubkey
	blockTime: Date | undefined
	instructionOrdinal: bigint
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
}

async function processAdminAction(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const runInfo = dataStore.getRunInfo(context.runAddress)
	runInfo.adminHistory.push(context)
	runInfo.adminHistory.sort((a, b) =>
		Number(b.instructionOrdinal - a.instructionOrdinal)
	)
}

async function processWitness(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const witnessPayload = witnessArgsJsonDecoder(context.instructionPayload)
	if (witnessPayload.metadata.evals !== undefined) {
		console.warn(
			'Coordinator: Ignoring metadata.evals in witness payload',
			witnessPayload.metadata.evals
		)
	}
	const runInfo = dataStore.getRunInfo(context.runAddress)
	const userWitnesses = runInfo.witnessesPerUser.get(context.signerAddress) ?? {
		lastFew: [],
		sampled: { rate: 1, data: [] },
	}
	const desiredLastFewCount = 10
	const desiredSampledCount = 100
	const witness = {
		blockTime: context.blockTime,
		ordinal: context.instructionOrdinal,
		proof: witnessPayload.proof,
		metadata: witnessPayload.metadata,
	}
	userWitnesses.lastFew.push(witness)
	userWitnesses.lastFew.sort((a, b) => Number(b.ordinal - a.ordinal))
	userWitnesses.lastFew = userWitnesses.lastFew.slice(0, desiredLastFewCount)
	const selector = Math.random()
	if (selector < 1 / userWitnesses.sampled.rate) {
		userWitnesses.sampled.data.push({ selector, witness })
		userWitnesses.sampled.data.sort((a, b) =>
			Number(b.witness.ordinal - a.witness.ordinal)
		)
		while (userWitnesses.sampled.data.length >= desiredSampledCount * 1.5) {
			userWitnesses.sampled.rate *= 1.5
			userWitnesses.sampled.data = userWitnesses.sampled.data.filter(
				(item) => item.selector < 1 / userWitnesses.sampled.rate
			)
		}
	}
	runInfo.witnessesPerUser.set(context.signerAddress, userWitnesses)
}

const witnessProofJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	position: jsonCodecInteger.decoder,
	index: jsonCodecInteger.decoder,
	witness: utilsRustSmallBooleanJsonDecoder,
})

const witnessMetadataJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	step: jsonCodecNumber.decoder,
	tokensPerSec: jsonCodecNumber.decoder,
	bandwidthPerSec: jsonCodecNumber.decoder,
	loss: jsonCodecNumber.decoder,
	evals: utilsRustFixedArrayJsonDecoder(
		jsonDecoderObjectWithKeysSnakeEncoded({
			name: utilsRustFixedStringJsonDecoder,
			value: jsonCodecNumber.decoder,
		})
	),
	promptResults: utilsRustFixedArrayJsonDecoder(jsonCodecNumber.decoder),
	promptIndex: jsonCodecNumber.decoder,
	efficiency: jsonCodecNumber.decoder,
})

const witnessArgsJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	proof: witnessProofJsonDecoder,
	metadata: witnessMetadataJsonDecoder,
})
