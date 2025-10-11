import {
	IdlProgram,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecPubkey,
	jsonCodecString,
	jsonDecoderObjectEncodedSnakeKeys,
	Pubkey,
	RpcHttp,
} from 'solana-kiss'
import {
	utilsGetAndDecodeAccountState,
	utilsRustFixedArrayJsonDecoder,
	utilsRustFixedStringJsonDecoder,
} from '../utils'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorIndexingCheckpoint(
	rpcHttp: RpcHttp,
	programIdl: IdlProgram,
	dataStore: CoordinatorDataStore
) {
	const promises = new Array<Promise<void>>()
	for (const [runAddress, runInfo] of dataStore.runInfoByAddress) {
		if (runInfo.accountFetchedOrdering === runInfo.accountRequestOrdering) {
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
		dataStore.saveRunState(runAddress, {
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
		})
	} catch (error) {
		console.error('Failed to refresh run state', runAddress, error)
	}
}

const runStateJsonDecoder = jsonDecoderObjectEncodedSnakeKeys({
	nonce: jsonCodecInteger.decoder,
	state: jsonDecoderObjectEncodedSnakeKeys({
		metadata: jsonDecoderObjectEncodedSnakeKeys({
			name: utilsRustFixedStringJsonDecoder,
			description: utilsRustFixedStringJsonDecoder,
			numParameters: jsonCodecInteger.decoder,
			vocabSize: jsonCodecInteger.decoder,
		}),
		coordinator: jsonDecoderObjectEncodedSnakeKeys({
			runId: utilsRustFixedStringJsonDecoder,
			runState: jsonCodecString.decoder,
			progress: jsonDecoderObjectEncodedSnakeKeys({
				epoch: jsonCodecNumber.decoder,
				step: jsonCodecNumber.decoder,
				epochStartDataIndex: jsonCodecInteger.decoder,
			}),
			epochState: jsonDecoderObjectEncodedSnakeKeys({
				clients: utilsRustFixedArrayJsonDecoder(
					jsonDecoderObjectEncodedSnakeKeys({
						id: jsonDecoderObjectEncodedSnakeKeys({
							signer: jsonCodecPubkey.decoder,
						}),
						state: jsonCodecString.decoder,
					})
				),
			}),
		}),
	}),
})
