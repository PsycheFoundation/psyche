import { Application } from 'express'
import {
	Pubkey,
	jsonCodecArray,
	jsonCodecInteger,
	jsonCodecObject,
	jsonCodecPubkey,
} from 'solana-kiss'
import { miningPoolDataPoolInfoJsonCodec } from './MiningPoolDataPoolInfo'
import { miningPoolDataPoolStateJsonCodec } from './MiningPoolDataPoolState'
import { MiningPoolDataStore } from './MiningPoolDataStore'

export async function miningPoolEndpoint(
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
		const poolAddress = dataStore.getPoolAddress(poolIndex)
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

const poolSummariesJsonCodec = jsonCodecArray(
	jsonCodecObject({
		address: jsonCodecPubkey,
		state: miningPoolDataPoolStateJsonCodec,
	})
)
