import { Application } from 'express'
import {
	jsonCodecArray,
	jsonCodecObject,
	jsonCodecPubkey,
	jsonCodecString,
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
import { coordinatorDataRunInfoJsonCodec } from './CoordinatorDataRunInfo'
import { coordinatorDataRunStateJsonCodec } from './CoordinatorDataRunState'
import {
	CoordinatorDataStore,
	coordinatorDataStoreJsonCodec,
} from './CoordinatorDataStore'
import { coordinatorIndexingCheckpoint } from './CoordinatorIndexingCheckpoint'
import { coordinatorIndexingInstruction } from './CoordinatorIndexingInstruction'

export async function coordinatorService(
	cluster: string,
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	expressApp: Application
) {
	const saveName = `coordinator_${cluster}_${programAddress}`
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
	let dataStore: CoordinatorDataStore
	try {
		const saveContent = await saveRead(saveName)
		checkpoint = indexingCheckpointJsonCodec.decoder(saveContent.checkpoint)
		dataStore = coordinatorDataStoreJsonCodec.decoder(saveContent.dataStore)
		console.log('Loaded coordinator state from:', saveContent.updatedAt)
	} catch (error) {
		checkpoint = { indexedChunks: [] }
		dataStore = new CoordinatorDataStore(new Map(), new Map())
		console.warn(
			'Failed to read existing coordinator JSON, starting fresh',
			error
		)
	}
	return { checkpoint, dataStore }
}

async function serviceEndpoint(
	programAddress: Pubkey,
	expressApp: Application,
	dataStore: CoordinatorDataStore
) {
	expressApp.get(`/coordinator/${programAddress}/summaries`, (_, res) => {
		const runSummaries = []
		for (const [runAddress, runInfo] of dataStore.runInfoByAddress) {
			const runState = runInfo?.accountState
			if (runState === undefined) {
				continue
			}
			runSummaries.push({ address: runAddress, state: runState })
		}
		return res.status(200).json(runSummariesJsonCodec.encoder(runSummaries))
	})
	expressApp.get(`/coordinator/${programAddress}/run/:runId`, (req, res) => {
		const runId = jsonCodecString.decoder(req.params.runId)
		const runAddress = dataStore.runAddressByRunId.get(runId)
		if (!runAddress) {
			return res.status(404).json({ error: 'Run address not found' })
		}
		const runInfo = dataStore.runInfoByAddress.get(runAddress)
		if (!runInfo) {
			return res.status(404).json({ error: 'Run info not found' })
		}
		return res
			.status(200)
			.json(coordinatorDataRunInfoJsonCodec.encoder(runInfo))
	})
}

async function serviceIndexing(
	saveName: string,
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	dataStore: CoordinatorDataStore
): Promise<void> {
	const programIdl = await utilsGetProgramAnchorIdl(rpcHttp, programAddress)
	await indexingInstructionsLoop(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		programIdl,
		async (
			instructionName,
			instructionAddresses,
			instructionPayload,
			context
		) => {
			await coordinatorIndexingInstruction(
				dataStore,
				instructionName,
				instructionAddresses,
				instructionPayload,
				context.ordering,
				context.transaction.execution.blockInfo.time
			)
		},
		async (checkpoint) => {
			await coordinatorIndexingCheckpoint(rpcHttp, programIdl, dataStore)
			await saveWrite(saveName, {
				checkpoint: indexingCheckpointJsonCodec.encoder(checkpoint),
				dataStore: coordinatorDataStoreJsonCodec.encoder(dataStore),
			})
		}
	)
}

const runSummariesJsonCodec = jsonCodecArray(
	jsonCodecObject({
		address: jsonCodecPubkey,
		state: coordinatorDataRunStateJsonCodec,
	})
)
