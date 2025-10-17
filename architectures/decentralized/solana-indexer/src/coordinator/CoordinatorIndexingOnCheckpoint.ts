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
	utilsGetAndDecodeAccountState,
	utilsRustClientIdJsonDecoder,
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
	utilsRustSmallBooleanJsonDecoder,
} from '../utils'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorIndexingOnCheckpoint(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	dataStore: CoordinatorDataStore
) {
	const promises = new Array<Promise<void>>()
	for (const [runAddress, runInfo] of dataStore.runInfoByAddress) {
		if (runInfo.accountFetchedOrdinal === runInfo.accountRequestOrdinal) {
			break
		}
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
		console.log('Fetched run state', runAccountState)
		const runInfo = dataStore.getRunInfo(runAddress)
		runInfo.accountState = {
			runId: runAccountState.state.coordinator.runId,
			mainAuthority: runInstanceState.mainAuthority,
			joinAuthority: runInstanceState.joinAuthority,
			name: runAccountState.state.metadata.name,
			description: runAccountState.state.metadata.description,
			numParameters: runAccountState.state.metadata.numParameters,
			status: runAccountState.state.coordinator.runState,
			model: runAccountState.state.coordinator.model,
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
		runInfo.accountUpdatedAt = new Date()
		runInfo.accountFetchedOrdinal = runInfo.accountRequestOrdinal
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
