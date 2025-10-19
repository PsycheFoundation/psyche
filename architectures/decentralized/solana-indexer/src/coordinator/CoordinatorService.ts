import { Application } from 'express'
import { Pubkey, RpcHttp } from 'solana-kiss'
import {
	IndexingCheckpoint,
	indexingCheckpointJsonCodec,
} from '../indexing/IndexingCheckpoint'
import { indexingInstructions } from '../indexing/IndexingInstructions'
import { saveRead, saveWrite } from '../save'
import { utilsGetProgramAnchorIdl } from '../utils'
import {
	CoordinatorDataStore,
	coordinatorDataStoreJsonCodec,
} from './CoordinatorDataStore'
import { coordinatorEndpoint } from './CoordinatorEndpoint'
import { coordinatorIndexingOnCheckpoint } from './CoordinatorIndexingOnCheckpoint'
import { coordinatorIndexingOnInstruction } from './CoordinatorIndexingOnInstruction'

export async function coordinatorService(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	expressApp: Application
) {
	const saveName = `${programAddress}_coordinator`
	const { checkpoint, dataStore } = await serviceLoader(
		saveName,
		programAddress
	)
	coordinatorEndpoint(programAddress, expressApp, dataStore)
	await serviceIndexing(
		saveName,
		rpcHttp,
		programAddress,
		checkpoint,
		dataStore
	)
}

async function serviceLoader(saveName: string, programAddress: Pubkey) {
	let checkpoint: IndexingCheckpoint
	let dataStore: CoordinatorDataStore
	try {
		const saveContent = await saveRead(saveName)
		checkpoint = indexingCheckpointJsonCodec.decoder(saveContent.checkpoint)
		dataStore = coordinatorDataStoreJsonCodec.decoder(saveContent.dataStore)
		console.log('Loaded coordinator state from:', saveContent.updatedAt)
	} catch (error) {
		if (!process.env['ALLOW_NEW_STATE_COORDINATOR']) {
			throw new Error(
				'Failed to read existing coordinator JSON, and ALLOW_NEW_STATE_COORDINATOR is not set'
			)
		}
		checkpoint = { orderedIndexedChunks: [] }
		dataStore = new CoordinatorDataStore(programAddress, new Map())
		console.warn(
			'Failed to read existing coordinator JSON, starting fresh',
			error
		)
	}
	return { checkpoint, dataStore }
}

async function serviceIndexing(
	saveName: string,
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	dataStore: CoordinatorDataStore
): Promise<void> {
	const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress)
	await indexingInstructions(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		programIdl,
		async ({
			blockTime,
			instructionOrdinal,
			instructionName,
			instructionAddresses,
			instructionPayload,
		}) => {
			await coordinatorIndexingOnInstruction(
				dataStore,
				blockTime,
				instructionOrdinal,
				instructionName,
				instructionAddresses,
				instructionPayload
			)
		},
		async (checkpoint) => {
			await coordinatorIndexingOnCheckpoint(rpcHttp, programIdl, dataStore)
			await saveWrite(saveName, {
				checkpoint: indexingCheckpointJsonCodec.encoder(checkpoint),
				dataStore: coordinatorDataStoreJsonCodec.encoder(dataStore),
			})
		}
	)
}
