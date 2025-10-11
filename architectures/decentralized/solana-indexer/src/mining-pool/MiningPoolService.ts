import {
	jsonCodecArray,
	jsonCodecInteger,
	jsonCodecObject,
	jsonCodecPubkey,
	Pubkey,
	RpcHttp,
} from 'solana-kiss'
import {
	IndexingCheckpoint,
	indexingCheckpointJsonCodec,
} from '../indexing/IndexingCheckpoint'
import { indexingInstructionsLoop } from '../indexing/IndexingInstructions'
import { saveRead, saveWrite } from '../save'
import { utilsGetProgramAnchorIdl } from '../utils'
import {
	MiningPoolDataStore,
	miningPoolDataStoreJsonCodec,
} from './MiningPoolDataStore'
import { miningPoolIndexingCheckpoint } from './MiningPoolIndexingCheckpoint'
import { miningPoolIndexingInstruction } from './MiningPoolIndexingInstruction'

import { Application } from 'express'
import { miningPoolDataPoolInfoJsonCodec } from './MiningPoolDataPoolInfo'
import { miningPoolDataPoolStateJsonCodec } from './MiningPoolDataPoolState'

export async function miningPoolService(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	expressApp: Application
): Promise<void> {
	const saveName = `mining_pool_${programAddress}`
	const { checkpoint, dataStore } = await serviceLoader(saveName)
	serviceEndpoint(programAddress, expressApp, dataStore)
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

async function serviceEndpoint(
	programAddress: Pubkey,
	expressApp: Application,
	dataStore: MiningPoolDataStore
) {
	expressApp.get(`/mining-pool/${programAddress}/summaries`, (_, res) => {
		const poolsSummaries = []
		for (const [poolAddress, poolInfo] of dataStore.poolInfoByAddress) {
			const poolState = poolInfo?.accountState
			if (poolState === undefined) {
				continue
			}
			poolsSummaries.push({ address: poolAddress, state: poolState })
		}
		return res.status(200).json(poolSummariesJsonCodec.encoder(poolsSummaries))
	})
	expressApp.get(`/mining-pool/${programAddress}/pool/:index`, (req, res) => {
		const poolIndex = jsonCodecInteger.decoder(req.params.index)
		const poolAddress = dataStore.poolAddressByIndex.get(poolIndex)
		if (!poolAddress) {
			return res.status(404).json({ error: 'Pool address not found' })
		}
		const poolInfo = dataStore.poolInfoByAddress.get(poolAddress)
		if (!poolInfo) {
			return res.status(404).json({ error: 'Pool info not found' })
		}
		return res
			.status(200)
			.json(miningPoolDataPoolInfoJsonCodec.encoder(poolInfo))
	})
}

async function serviceIndexing(
	saveName: string,
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	dataStore: MiningPoolDataStore
) {
	const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress)
	await indexingInstructionsLoop(
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
			await miningPoolIndexingInstruction(
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

const poolSummariesJsonCodec = jsonCodecArray(
	jsonCodecObject({
		address: jsonCodecPubkey,
		state: miningPoolDataPoolStateJsonCodec,
	})
)
