import express from 'express'
import {
	pubkeyFromBase58,
	rpcHttpFromUrl,
	rpcHttpWithMaxConcurrentRequests,
	rpcHttpWithRetryOnError,
} from 'solana-kiss'
import { coordinatorService } from './coordinator/CoordinatorService'
import { miningPoolService } from './mining-pool/MiningPoolService'
import { utilsGetEnv } from './utils'

function rpcHttpBuilder(url: string) {
	return rpcHttpWithRetryOnError(
		rpcHttpWithMaxConcurrentRequests(
			rpcHttpFromUrl(url, { commitment: 'confirmed' }),
			100
		),
		async (error, context) => {
			if (context.totalDurationMs > 10_000) {
				console.error('Rpc failed to reply for too long, giving up', error)
				return false
			}
			await new Promise((resolve) => setTimeout(resolve, 100))
			return true
		}
	)
}

async function main() {
	const expressApp = express()
	const httpApiPort = process.env['PORT'] ?? 3000
	expressApp.listen(httpApiPort, (error) => {
		if (error) {
			console.error('Error starting server:', error)
		} else {
			console.log(`Listening on port ${httpApiPort}`)
		}
	})
	miningPoolService(
		rpcHttpBuilder(utilsGetEnv('MINING_POOL_RPC', 'Mining Pool RPC url')),
		pubkeyFromBase58('PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf'),
		expressApp
	)
	coordinatorService(
		rpcHttpBuilder(utilsGetEnv('COORDINATOR_RPC', 'Coordinator RPC url')),
		pubkeyFromBase58('HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y'),
		expressApp
	)
}

main()
