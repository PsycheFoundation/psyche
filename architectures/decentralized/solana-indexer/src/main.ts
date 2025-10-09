import express from 'express'
import {
	pubkeyFromBase58,
	rpcHttpFromUrl,
	rpcHttpWithRetryOnError,
} from 'solana-kiss'
import { coordinatorService } from './coordinator/CoordinatorService'
import { miningPoolService } from './mining-pool/MiningPoolService'

function rpcHttpBuilder(url: string) {
	return rpcHttpWithRetryOnError(
		rpcHttpFromUrl(url, { commitment: 'confirmed' }),
		async (error, context) => {
			if (context.retriedCounter > 10) {
				return false
			}
			await new Promise((resolve) => setTimeout(resolve, 1000))
			console.error('RPC HTTP error occurred, retrying', error)
			return true
		}
	)
}

function getEnvVariable(name: string, description: string): string {
	const val = process.env[name]
	if (!val) {
		throw new Error(`Missing ${description} in environment: ${name}`)
	}
	return val
}

const miningPoolRpc = getEnvVariable('MINING_POOL_RPC', 'Mining Pool RPC url')

const miningPoolCluster = 'mainnet'
const miningPoolRpcHttp = rpcHttpBuilder(miningPoolRpc)
const miningPoolProgramAddress = pubkeyFromBase58(
	'PsyMP8fXEEMo2C6C84s8eXuRUrvzQnZyquyjipDRohf'
)

const coordinatorRpc = getEnvVariable('COORDINATOR_RPC', 'Coordinator RPC url')

const coordinatorCluster = 'devnet'
const coordinatorRpcHttp = rpcHttpBuilder(coordinatorRpc)
const coordinatorProgramAddress = pubkeyFromBase58(
	'HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y'
)

const expressApp = express()

async function coordinatorMain() {
	coordinatorService(
		coordinatorCluster,
		coordinatorRpcHttp,
		coordinatorProgramAddress,
		expressApp
	)
}

async function miningPoolMain() {
	miningPoolService(
		miningPoolCluster,
		miningPoolRpcHttp,
		miningPoolProgramAddress,
		expressApp
	)
}

coordinatorMain()
miningPoolMain()

const httpApiPort = process.env['PORT'] ?? 3000
expressApp.listen(httpApiPort, (error) => {
	if (error) {
		console.error('Error starting server:', error)
	} else {
		console.log(`Listening on port ${httpApiPort}`)
	}
})
