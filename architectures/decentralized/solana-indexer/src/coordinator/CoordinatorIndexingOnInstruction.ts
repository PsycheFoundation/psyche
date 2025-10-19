import {
	JsonValue,
	Pubkey,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonDecoderObjectWithKeysSnakeEncoded,
	jsonDecoderOptional,
} from 'solana-kiss'
import {
	utilsBigintArraySortAscending,
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
	utilsRustSmallBooleanJsonDecoder,
} from '../utils'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorIndexingOnInstruction(
	dataStore: CoordinatorDataStore,
	blockTime: Date | undefined,
	instructionOrdinal: bigint,
	instructionName: string,
	instructionAddresses: Map<string, Pubkey>,
	instructionPayload: JsonValue
): Promise<void> {
	const runAddress = instructionAddresses.get('coordinator_instance')
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
	if (instructionOrdinal > runInfo.changeNotificationOrdinal) {
		runInfo.changeNotificationOrdinal = instructionOrdinal
	}
}

const processorsByInstructionName = new Map([
	['init_coordinator', [processImportantAction]],
	['update', [processImportantAction]],
	['set_future_epoch_rates', [processImportantAction]],
	['set_paused', [processImportantAction]],
	['join_run', [processImportantAction]],
	['warmup_witness', []], // TODO - how to handle warmup witness?
	['witness', [processWitness]],
	['tick', []],
	['checkpoint', [processImportantAction]],
	['health_check', []], // TODO - how to handle health check?
	['free_coordinator', [processImportantAction, processFinish]],
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

async function processImportantAction(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const runInfo = dataStore.getRunInfo(context.runAddress)
	runInfo.importantHistory.push(context)
	utilsBigintArraySortAscending(
		runInfo.importantHistory,
		(importantAction) => importantAction.instructionOrdinal
	)
	runInfo.importantHistory.reverse()
}

async function processFinish(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const runInfo = dataStore.getRunInfo(context.runAddress)
	runInfo.finishesOrdinals.push(context.instructionOrdinal)
}

async function processWitness(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const runInfo = dataStore.getRunInfo(context.runAddress)
	const witnessPayload = witnessJsonDecoder(context.instructionPayload)
	if (!witnessPayload.proof.witness) {
		return
	}
	const witnessUser = context.signerAddress
	const witnessOrdinal = context.instructionOrdinal
	const witnessStep = witnessPayload.metadata.step
	const lastWitnessForUser = runInfo.lastWitnessByUser.get(witnessUser)
	if (
		lastWitnessForUser === undefined ||
		lastWitnessForUser.ordinal < witnessOrdinal
	) {
		runInfo.lastWitnessByUser.set(witnessUser, {
			ordinal: witnessOrdinal,
			step: witnessStep,
		})
	}
	const witnessStats = new Map<string, number>()
	if (witnessPayload.metadata.bandwidthPerSec !== undefined) {
		witnessStats.set('bandwidthPerSec', witnessPayload.metadata.bandwidthPerSec)
	}
	if (witnessPayload.metadata.tokensPerSec !== undefined) {
		witnessStats.set('tokensPerSec', witnessPayload.metadata.tokensPerSec)
	}
	if (witnessPayload.metadata.efficiency !== undefined) {
		witnessStats.set('efficiency', witnessPayload.metadata.efficiency)
	}
	if (witnessPayload.metadata.loss !== undefined) {
		witnessStats.set('loss', witnessPayload.metadata.loss)
	}
	if (witnessPayload.metadata.evals !== undefined) {
		for (const evalItem of witnessPayload.metadata.evals) {
			witnessStats.set(evalItem.name, evalItem.value)
		}
	}
	for (const [statName, statValue] of witnessStats.entries()) {
		if (!isFinite(statValue)) {
			continue
		}
		let statSamples = runInfo.samplesByStatName.get(statName)
		if (statSamples === undefined) {
			statSamples = []
			runInfo.samplesByStatName.set(statName, statSamples)
		}
		statSamples.push({
			maxOrdinal: witnessOrdinal,
			step: witnessStep,
			sumValue: statValue,
			numValue: 1,
			time: context.blockTime,
		})
	}
}

const witnessProofJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	position: jsonCodecInteger.decoder,
	index: jsonCodecInteger.decoder,
	witness: utilsRustSmallBooleanJsonDecoder,
})

const witnessMetadataJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	step: jsonCodecNumber.decoder,
	tokensPerSec: jsonDecoderOptional(jsonCodecNumber.decoder),
	bandwidthPerSec: jsonDecoderOptional(jsonCodecNumber.decoder),
	efficiency: jsonDecoderOptional(jsonCodecNumber.decoder),
	loss: jsonDecoderOptional(jsonCodecNumber.decoder),
	promptIndex: jsonDecoderOptional(jsonCodecNumber.decoder),
	promptResults: jsonDecoderOptional(
		utilsRustFixedArrayJsonDecoder(jsonCodecNumber.decoder)
	),
	evals: jsonDecoderOptional(
		utilsRustFixedArrayJsonDecoder(
			jsonDecoderObjectWithKeysSnakeEncoded({
				name: utilsRustFixedStringJsonDecoder,
				value: jsonCodecNumber.decoder,
			})
		)
	),
})

const witnessJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	proof: witnessProofJsonDecoder,
	metadata: witnessMetadataJsonDecoder,
})
