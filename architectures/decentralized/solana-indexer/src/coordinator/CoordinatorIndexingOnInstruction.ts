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
	['free_coordinator', [processAdminAction, processFinish]],
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
	utilsBigintArraySortAscending(
		runInfo.adminHistory,
		(adminAction) => adminAction.instructionOrdinal
	)
	runInfo.adminHistory.reverse()
}

async function processFinish(
	dataStore: CoordinatorDataStore,
	context: ProcessingContext
): Promise<void> {
	const runInfo = dataStore.getRunInfo(context.runAddress)
	runInfo.finishesOrdinals.push(context.instructionOrdinal)
	utilsBigintArraySortAscending(runInfo.finishesOrdinals, (ordinal) => ordinal)
	console.log('slicesOrdinals', context.runAddress, runInfo.finishesOrdinals)
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
	const witnessTime = context.blockTime
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
		let statSamples = runInfo.samplesByStatName.get(statName)
		if (statSamples === undefined) {
			statSamples = []
			runInfo.samplesByStatName.set(statName, statSamples)
		}
		statSamples.push({
			minTime: witnessTime,
			maxTime: witnessTime,
			minOrdinal: witnessOrdinal,
			maxOrdinal: witnessOrdinal,
			minStep: witnessStep,
			maxStep: witnessStep,
			minValue: statValue,
			maxValue: statValue,
			sumValue: statValue,
			numValue: 1,
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
