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
	utilsGetAndDecodeAccountState,
	utilsPlotPoints,
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
			aggregateStatSamples(
				runInfo.accountState?.runId,
				runAddress,
				statName,
				statSamples
			)
		}
		if (runInfo.finishesOrdinals.length > 0) {
			console.log('Finishes ordinals:', runAddress, runInfo.finishesOrdinals)
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

function aggregateStatSamples(
	_runId: string | undefined,
	_runAddress: Pubkey,
	_statName: string,
	statSamples: Array<CoordinatorDataRunInfoSample>
) {
	utilsBigintArraySortAscending(statSamples, (sample) => sample.maxOrdinal)
	for (
		let sampleIndex = statSamples.length - 2;
		sampleIndex >= 0;
		sampleIndex--
	) {
		const prevIndex = sampleIndex
		const nextIndex = sampleIndex + 1
		const prevSample = statSamples[prevIndex]!
		const nextSample = statSamples[nextIndex]!
		if (prevSample.step === nextSample.step) {
			nextSample.sumValue += prevSample.sumValue
			nextSample.numValue += prevSample.numValue
			statSamples.splice(prevIndex, 1)
		}
	}
	utilsPlotPoints(
		`${_runId ? _runId : _runAddress} (${_statName})`,
		{ x: 90, y: 10 },
		statSamples.map((sample, _index) => ({
			x: sample.step,
			y: sample.sumValue / sample.numValue,
		}))
	)
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
