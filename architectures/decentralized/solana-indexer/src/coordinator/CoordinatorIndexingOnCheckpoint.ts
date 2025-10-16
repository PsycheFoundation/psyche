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
		runInfo.accountState = {
			runId: runAccountState.state.coordinator.runId,
			name: runAccountState.state.metadata.name,
			description: runAccountState.state.metadata.description,
			status: runAccountState.state.coordinator.runState,
			epochClients: runAccountState.state.coordinator.epochState.clients.map(
				(client) => ({
					signer: client.id.signer,
					state: client.state,
				})
			),
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
	runId: utilsRustFixedStringJsonDecoder,
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
