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
	MiningPoolDataStore,
	miningPoolDataStoreJsonCodec,
} from './MiningPoolDataStore'
import { miningPoolEndpoint } from './MiningPoolEndpoint'
import { miningPoolIndexingCheckpoint } from './MiningPoolIndexingOnCheckpoint'
import { miningPoolIndexingOnInstruction } from './MiningPoolIndexingOnInstruction'

export async function miningPoolService(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	expressApp: Application
): Promise<void> {
	const saveName = `mining_pool_${programAddress}`
	const { checkpoint, dataStore } = await serviceLoader(saveName)
	miningPoolEndpoint(programAddress, expressApp, dataStore)
	await serviceIndexing(
		saveName,
		rpcHttp,
		programAddress,
		checkpoint,
		dataStore
	)
}

async function serviceLoader(saveName: string) {
	let checkpoint: IndexingCheckpoint
	let dataStore: MiningPoolDataStore
	try {
		const saveContent = await saveRead(saveName)
		checkpoint = indexingCheckpointJsonCodec.decoder(saveContent.checkpoint)
		dataStore = miningPoolDataStoreJsonCodec.decoder(saveContent.dataStore)
		console.log('Loaded mining pool state from:', saveContent.updatedAt)
	} catch (error) {
		checkpoint = { orderedIndexedChunks: [] }
		dataStore = new MiningPoolDataStore(new Map(), new Map())
		console.warn(
			'Failed to read existing mining pool JSON, starting fresh',
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
	dataStore: MiningPoolDataStore
) {
	const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress)
	await indexingInstructions(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		programIdl,
		async ({
			blockTime,
			instructionName,
			instructionAddresses,
			instructionPayload,
			instructionOrdinal,
		}) => {
			await miningPoolIndexingOnInstruction(
				dataStore,
				blockTime,
				instructionName,
				instructionAddresses,
				instructionPayload,
				instructionOrdinal
			)
		},
		async (checkpoint) => {
			await miningPoolIndexingCheckpoint(rpcHttp, programIdl, dataStore)
			await saveWrite(saveName, {
				checkpoint: indexingCheckpointJsonCodec.encoder(checkpoint),
				dataStore: miningPoolDataStoreJsonCodec.encoder(dataStore),
			})
		}
	)
}
