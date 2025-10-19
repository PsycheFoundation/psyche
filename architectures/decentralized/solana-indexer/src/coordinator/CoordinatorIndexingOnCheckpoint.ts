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
	let dataSize = new Map<Pubkey, Map<string, number>>()
	for (const [runAddress, runInfo] of dataStore.runInfoByAddress) {
		const runDataSize = new Map<string, number>()
		for (const [statName, statSamples] of runInfo.samplesByStatName) {
			const dataKey = `stat_samples:${statName}`
			runDataSize.set(dataKey, statSamples.length)
		}
		for (const importantAction of runInfo.importantHistory) {
			const dataKey = `important_action:${importantAction.instructionName}`
			const currentSize = runDataSize.get(dataKey) ?? 0
			runDataSize.set(dataKey, currentSize + 1)
		}
		dataSize.set(runAddress, runDataSize)
	}
	console.log('Total collected data:', dataSize)
}

function aggregateStatSamples(
	runName: string | undefined,
	runAddress: Pubkey,
	statName: string,
	statSamples: Array<CoordinatorDataRunInfoSample>,
	finishesOrdinals: Array<bigint>
) {
	utilsBigintArraySortAscending(statSamples, (sample) => sample.maxOrdinal)
	utilsBigintArraySortAscending(
		finishesOrdinals,
		(finishOrdinal) => finishOrdinal
	)
	for (
		let sliceIndex = 0;
		sliceIndex <= finishesOrdinals.length;
		sliceIndex++
	) {
		const prevOrdinal = finishesOrdinals[sliceIndex - 1]
		const nextOrdinal = finishesOrdinals[sliceIndex]
		let sampleIndexMin = 0
		if (prevOrdinal !== undefined) {
			while (
				sampleIndexMin < statSamples.length &&
				statSamples[sampleIndexMin]!.maxOrdinal < prevOrdinal
			) {
				sampleIndexMin++
			}
		}
		let sampleIndexMax = statSamples.length - 1
		if (nextOrdinal !== undefined) {
			while (
				sampleIndexMax >= sampleIndexMin &&
				statSamples[sampleIndexMax]!.maxOrdinal > nextOrdinal
			) {
				sampleIndexMax--
			}
		}
		aggregateStatSamplesSlice(
			runName,
			runAddress,
			statName,
			sliceIndex,
			statSamples,
			sampleIndexMin,
			sampleIndexMax
		)
	}
	if (statSamples.length > 100) {
		utilsPlotPoints(
			`${runName ? runName : runAddress} history (${statName})`,
			statSamples.map((sample) => ({
				x: sample.time?.getTime() ?? NaN,
				y: sample.sumValue / sample.numValue,
			})),
			(x) => new Date(x).toISOString()
		)
	}
}

function aggregateStatSamplesSlice(
	runName: string | undefined,
	runAddress: Pubkey,
	statName: string,
	statSlice: number,
	statSamples: Array<CoordinatorDataRunInfoSample>,
	indexMin: number,
	indexMax: number
) {
	let maxStep = 0
	for (let sampleIndex = indexMax - 2; sampleIndex >= indexMin; sampleIndex--) {
		const prevIndex = sampleIndex
		const nextIndex = sampleIndex + 1
		const prevSample = statSamples[prevIndex]!
		const nextSample = statSamples[nextIndex]!
		maxStep = Math.max(maxStep, nextSample.step)
		if (prevSample.step === nextSample.step) {
			nextSample.sumValue += prevSample.sumValue
			nextSample.numValue += prevSample.numValue
			statSamples.splice(prevIndex, 1)
			indexMax--
		}
	}
	let chunkSize = 1
	while (chunkSize * 1000 < maxStep) {
		chunkSize *= 2
	}
	for (let sampleIndex = indexMax - 1; sampleIndex >= indexMin; sampleIndex--) {
		if (statSamples[sampleIndex]!.step % chunkSize !== 0) {
			statSamples.splice(sampleIndex, 1)
			indexMax--
		}
	}
	const statSamplesSlice = statSamples.slice(indexMin, indexMax + 1)
	if (statSamplesSlice.length > 100) {
		utilsPlotPoints(
			`${runName ? runName : runAddress} slice${statSlice} (${statName})`,
			statSamplesSlice.map((sample) => ({
				x: sample.step,
				y: sample.sumValue / sample.numValue,
			})),
			(x) => `Step ${x}`
		)
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
			joinedClients: runAccountState.state.clientsState.clients.map(
				(client) => ({
					signer: client.id.signer,
					earned: client.earned,
					slashed: client.slashed,
				})
			),
			epochClients: runAccountState.state.coordinator.epochState.clients.map(
				(client) => ({
					signer: client.id.signer,
					state: client.state,
				})
			),
			epochRates: {
				current: runAccountState.state.clientsState.currentEpochRates,
				future: runAccountState.state.clientsState.futureEpochRates,
			},
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
