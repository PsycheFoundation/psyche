import {
	IdlProgram,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecPubkey,
	jsonCodecString,
	jsonDecoderObjectWithKeysSnakeEncoded,
	Pubkey,
	RpcHttp,
} from 'solana-kiss'
import {
	utilsGetAndDecodeAccountState,
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
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
		const runState = await utilsGetAndDecodeAccountState(
			rpcHttp,
			programIdl,
			runAddress,
			runStateJsonDecoder
		)
		const runInfo = dataStore.getRunInfo(runAddress)
		runInfo.accountState = {
			runId: runState.state.coordinator.runId,
			name: runState.state.metadata.name,
			description: runState.state.metadata.description,
			status: runState.state.coordinator.runState,
			epochClients: runState.state.coordinator.epochState.clients.map(
				(client) => ({
					signer: client.id.signer,
					state: client.state,
				})
			),
			nonce: runState.nonce,
		}
		runInfo.accountUpdatedAt = new Date()
		runInfo.accountFetchedOrdinal = runInfo.accountRequestOrdinal
		dataStore.runAddressByRunId.set(
			runState.state.coordinator.runId,
			runAddress
		)
	} catch (error) {
		console.error('Failed to refresh run state', runAddress, error)
	}
}

const runStateJsonDecoder = jsonDecoderObjectWithKeysSnakeEncoded({
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
			progress: jsonDecoderObjectWithKeysSnakeEncoded({
				epoch: jsonCodecNumber.decoder,
				step: jsonCodecNumber.decoder,
				epochStartDataIndex: jsonCodecInteger.decoder,
			}),
			epochState: jsonDecoderObjectWithKeysSnakeEncoded({
				clients: utilsRustFixedArrayJsonDecoder(
					jsonDecoderObjectWithKeysSnakeEncoded({
						id: jsonDecoderObjectWithKeysSnakeEncoded({
							signer: jsonCodecPubkey.decoder,
						}),
						state: jsonCodecString.decoder,
					})
				),
			}),
		}),
	}),
})
