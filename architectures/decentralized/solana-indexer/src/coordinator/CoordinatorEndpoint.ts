import { Application } from 'express'
import {
	Pubkey,
	jsonCodecArray,
	jsonCodecObject,
	jsonCodecPubkey,
	jsonCodecString,
} from 'solana-kiss'
import { coordinatorDataRunInfoJsonCodec } from './CoordinatorDataRunInfo'
import { coordinatorDataRunStateJsonCodec } from './CoordinatorDataRunState'
import { CoordinatorDataStore } from './CoordinatorDataStore'

export async function coordinatorEndpoint(
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

const runSummariesJsonCodec = jsonCodecArray(
	jsonCodecObject({
		address: jsonCodecPubkey,
		state: coordinatorDataRunStateJsonCodec,
	})
)
