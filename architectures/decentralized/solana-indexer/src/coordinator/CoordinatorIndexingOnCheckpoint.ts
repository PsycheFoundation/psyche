import {
	IdlProgram,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecPubkey,
	jsonCodecRaw,
	jsonCodecString,
	jsonDecoderObjectWithKeysSnakeEncoded,
	Pubkey,
	RpcHttp,
} from 'solana-kiss'
import {
	utilsBigintArraySortAscending,
	utilsBigIntMax,
	utilsBigIntMin,
	utilsGetAndDecodeAccountState,
	utilsRustClientIdJsonDecoder,
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
	utilsRustSmallBooleanJsonDecoder,
} from '../utils'
import { CoordinatorDataRunInfoSample } from './CoordinatorDataRunInfo'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorIndexingOnCheckpoint(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	dataStore: CoordinatorDataStore
) {
	const promises = new Array<Promise<void>>()
	for (const [runAddress, runInfo] of dataStore.runInfoByAddress) {
		for (const [statName, statSamples] of runInfo.samplesByStatName) {
			cleanupSamples(
				runAddress,
				statName,
				statSamples,
				runInfo.finishesOrdinals
			)
		}
		if (
			runInfo.changeAcknowledgedOrdinal === runInfo.changeNotificationOrdinal
		) {
			continue
		}
		runInfo.changeAcknowledgedOrdinal = runInfo.changeNotificationOrdinal
		const promise = updateCoordinatorAccountState(
			rpcHttp,
			programIdl,
			dataStore,
			runAddress
		)
		promises.push(promise)
	}
	await Promise.all(promises)
}

function cleanupSamples(
	runAddress: Pubkey,
	statName: string,
	statSamples: Array<CoordinatorDataRunInfoSample>,
	_finishesOrdinals: Array<bigint>
) {
	utilsBigintArraySortAscending(statSamples, (sample) => sample.maxOrdinal)
	// TODO - split samples arrays by finishes ordinals (which allows discarding rewritten samples?)
	mergeSamples(runAddress, statName, statSamples)
}

function mergeSamples(
	_runAddress: Pubkey,
	_statName: string,
	statSamples: Array<CoordinatorDataRunInfoSample>
) {
	let mergeChunkSteps = 1
	while (true) {
		for (
			let sampleIndex = statSamples.length - 2;
			sampleIndex >= 0;
			sampleIndex--
		) {
			const prevIndex = sampleIndex
			const nextIndex = sampleIndex + 1
			const prevSample = statSamples[prevIndex]!
			const nextSample = statSamples[nextIndex]!
			const prevSampleChunk = Math.floor(prevSample.maxStep / mergeChunkSteps)
			const nextSampleChunk = Math.floor(nextSample.maxStep / mergeChunkSteps)
			if (
				prevSampleChunk === nextSampleChunk &&
				prevSample.maxStep <= nextSample.maxStep
			) {
				prevSample.minTime = prevSample.minTime
				prevSample.maxTime = nextSample.maxTime
				prevSample.minOrdinal = utilsBigIntMin(
					prevSample.minOrdinal,
					nextSample.minOrdinal
				)
				prevSample.maxOrdinal = utilsBigIntMax(
					prevSample.maxOrdinal,
					nextSample.maxOrdinal
				)
				prevSample.minStep = Math.min(prevSample.minStep, nextSample.minStep)
				prevSample.maxStep = Math.max(prevSample.maxStep, nextSample.maxStep)
				prevSample.minValue = Math.min(prevSample.minValue, nextSample.minValue)
				prevSample.maxValue = Math.max(prevSample.maxValue, nextSample.maxValue)
				prevSample.sumValue += nextSample.sumValue
				prevSample.numValue += nextSample.numValue
				statSamples.splice(nextIndex, 1)
			}
		}
		if (statSamples.length < 20) {
			if (
				_runAddress === 'BKDGHzM1ZvaVk4fgpATRcQCDaEctG3VFENp2TsPi2NDr' &&
				_statName === 'loss'
			) {
				console.log(
					'Final samples',
					_runAddress,
					_statName,
					statSamples.length,
					statSamples.map((s) => ({
						steps: `${s.minStep} -> ${s.maxStep} (x${s.maxStep - s.minStep})`,
						num: s.numValue,
						avg: s.sumValue / s.numValue,
					}))
				)
			}
			return
		}
		mergeChunkSteps *= 2
	}
}

async function updateCoordinatorAccountState(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	dataStore: CoordinatorDataStore,
	runAddress: Pubkey
) {
	try {
		const runInstanceState = await utilsGetAndDecodeAccountState(
			rpcHttp,
			programIdl,
			runAddress,
			runInstanceJsonDecoder
		)
		const runAccountAddress = runInstanceState.coordinatorAccount
		const runAccountState = await utilsGetAndDecodeAccountState(
			rpcHttp,
			programIdl,
			runAccountAddress,
			runAccountJsonDecoder
		)
		// console.log('Fetched run state', runAccountState)
		const runInfo = dataStore.getRunInfo(runAddress)
		runInfo.accountUpdatedAt = new Date()
		runInfo.accountState = {
			runId: runAccountState.state.coordinator.runId,
			coordinatorInstanceAddress: runAddress,
			coordinatorAccountAddress: runAccountAddress,
			mainAuthority: runInstanceState.mainAuthority,
			joinAuthority: runInstanceState.joinAuthority,
			name: runAccountState.state.metadata.name,
			description: runAccountState.state.metadata.description,
			status: runAccountState.state.coordinator.runState,
			model: runAccountState.state.coordinator.model,
			numParameters: runAccountState.state.metadata.numParameters,
			epochClients: runAccountState.state.coordinator.epochState.clients.map(
				(client) => ({
					signer: client.id.signer,
					state: client.state,
				})
			),
			progress: {
				epoch: runAccountState.state.coordinator.progress.epoch,
				step: runAccountState.state.coordinator.progress.step,
			},
			nonce: runAccountState.nonce,
		}
	} catch (error) {
		console.error('Failed to refresh run state', runAddress, error)
	}
}

const runInstanceJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	bump: jsonCodecNumber.decoder,
	mainAuthority: jsonCodecPubkey.decoder,
	joinAuthority: jsonCodecPubkey.decoder,
	coordinatorAccount: jsonCodecPubkey.decoder,
	runId: jsonCodecString.decoder,
})

const runAccountJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
	nonce: jsonCodecInteger.decoder,
	state: jsonDecoderObjectWithKeysSnakeEncoded({
		metadata: jsonDecoderObjectWithKeysSnakeEncoded({
			name: utilsRustFixedStringJsonDecoder,
			description: utilsRustFixedStringJsonDecoder,
			numParameters: jsonCodecInteger.decoder,
			vocabSize: jsonCodecInteger.decoder,
		}),
		coordinator: jsonDecoderObjectWithKeysSnakeEncoded({
			runId: utilsRustFixedStringJsonDecoder,
			runState: jsonCodecString.decoder,
			model: jsonCodecRaw.decoder,
			config: jsonDecoderObjectWithKeysSnakeEncoded({
				warmupTime: jsonCodecInteger.decoder,
				cooldownTime: jsonCodecInteger.decoder,
				maxRoundTrainTime: jsonCodecInteger.decoder,
				roundWitnessTime: jsonCodecInteger.decoder,
				globalBatchSizeWarmupTokens: jsonCodecInteger.decoder,
				roundsPerEpoch: jsonCodecNumber.decoder,
				totalSteps: jsonCodecNumber.decoder,
				initMinClients: jsonCodecNumber.decoder,
				minClients: jsonCodecNumber.decoder,
				witnessNodes: jsonCodecNumber.decoder,
				globalBatchSizeStart: jsonCodecNumber.decoder,
				globalBatchSizeEnd: jsonCodecNumber.decoder,
				verificationPercent: jsonCodecNumber.decoder,
			}),
			progress: jsonDecoderObjectWithKeysSnakeEncoded({
				epoch: jsonCodecNumber.decoder,
				step: jsonCodecNumber.decoder,
				epochStartDataIndex: jsonCodecInteger.decoder,
			}),
			epochState: jsonDecoderObjectWithKeysSnakeEncoded({
				rounds: jsonCodecRaw.decoder,
				clients: utilsRustFixedArrayJsonDecoder(
					jsonDecoderObjectWithKeysSnakeEncoded({
						id: utilsRustClientIdJsonDecoder,
						exitedHeight: jsonCodecNumber.decoder,
						state: jsonCodecString.decoder,
					})
				),
				exitedClients: utilsRustFixedArrayJsonDecoder(
					jsonDecoderObjectWithKeysSnakeEncoded({
						id: utilsRustClientIdJsonDecoder,
						exitedHeight: jsonCodecNumber.decoder,
						state: jsonCodecString.decoder,
					})
				),
				roundsHead: jsonCodecNumber.decoder,
				startStep: jsonCodecNumber.decoder,
				firstRound: utilsRustSmallBooleanJsonDecoder,
				checkpointed: utilsRustSmallBooleanJsonDecoder,
				coldStartEpoch: utilsRustSmallBooleanJsonDecoder,
			}),
			runStateStartUnixTimestamp: jsonCodecInteger.decoder,
			pendingPause: utilsRustSmallBooleanJsonDecoder,
		}),
		clientsState: jsonDecoderObjectWithKeysSnakeEncoded({
			nextActive: jsonCodecInteger.decoder,
			clients: utilsRustFixedArrayJsonDecoder(
				jsonDecoderObjectWithKeysSnakeEncoded({
					active: jsonCodecInteger.decoder,
					earned: jsonCodecInteger.decoder,
					slashed: jsonCodecInteger.decoder,
					id: utilsRustClientIdJsonDecoder,
				})
			),
			currentEpochRates: jsonDecoderObjectWithKeysSnakeEncoded({
				earningRate: jsonCodecInteger.decoder,
				slashingRate: jsonCodecInteger.decoder,
			}),
			futureEpochRates: jsonDecoderObjectWithKeysSnakeEncoded({
				earningRate: jsonCodecInteger.decoder,
				slashingRate: jsonCodecInteger.decoder,
			}),
		}),
		isWarmupFirstTick: utilsRustSmallBooleanJsonDecoder,
		isTrainingFirstTick: utilsRustSmallBooleanJsonDecoder,
	}),
})
