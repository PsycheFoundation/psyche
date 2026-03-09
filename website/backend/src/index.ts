import { startIndexingChainToDataStores } from './chainTracker.js'

import Fastify, { FastifyRequest } from 'fastify'
import cors from '@fastify/cors'

import {
	ApiGetContributionInfo,
	ApiGetRun,
	ApiGetRuns,
	coordinatorIdl,
	formats,
	IndexerStatus,
	miningPoolIdl,
	RunData,
} from 'shared'
import { Connection } from '@solana/web3.js'
import { makeRateLimitedFetch } from './rateLimit.js'
import { PassThrough } from 'node:stream'
import { getRunFromKey, runKey, UniqueRunKey } from './coordinator.js'
import { CURRENT_VERSION } from 'shared/formats/type.js'
import { RunSummariesData } from './dataStore.js'

const requiredEnvVars = ['COORDINATOR_RPC', 'MINING_POOL_RPC'] as const

const replacer = formats[CURRENT_VERSION].replacer

async function main() {
	for (const v of requiredEnvVars) {
		if (!process.env[v]) {
			throw new Error(`env var ${v} is not set.`)
		}
	}

	if (
		process.env.COORDINATOR_MIN_SLOT !== undefined &&
		`${Number.parseInt(process.env.COORDINATOR_MIN_SLOT, 10)}` !==
			process.env.COORDINATOR_MIN_SLOT
	) {
		throw new Error(
			`COORDINATOR_MIN_SLOT is not a valid integer! got ${process.env.COORDINATOR_MIN_SLOT}`
		)
	}

	if (
		process.env.MINING_POOL_MIN_SLOT !== undefined &&
		`${Number.parseInt(process.env.MINING_POOL_MIN_SLOT, 10)}` !==
			process.env.MINING_POOL_MIN_SLOT
	) {
		throw new Error(
			`MINING_POOL_MIN_SLOT is not a valid integer! got ${process.env.MINING_POOL_MIN_SLOT}`
		)
	}

	const coordinatorRpc = new Connection(process.env.COORDINATOR_RPC!, {
		fetch: makeRateLimitedFetch(),
	})

	// if the RPCs are the same, use only one to share rate limits.
	const miningPoolRpc =
		process.env.COORDINATOR_RPC === process.env.MINING_POOL_RPC
			? coordinatorRpc
			: new Connection(process.env.MINING_POOL_RPC!, {
					fetch: makeRateLimitedFetch(),
				})

	const { coordinator, miningPool, cancel } =
		await startIndexingChainToDataStores(
			{
				connection: coordinatorRpc,
				addressOverride: process.env.COORDINATOR_PROGRAM_ID,
				websocketRpcUrl: process.env.COORDINATOR_WS_RPC,
				minSlot: Number.parseInt(process.env.COORDINATOR_MIN_SLOT ?? '0'),
			},
			{
				connection: miningPoolRpc,
				addressOverride: process.env.MINING_POOL_PROGRAM_ID,
				websocketRpcUrl: process.env.MINING_POOL_WS_RPC,
				minSlot: Number.parseInt(process.env.MINING_POOL_MIN_SLOT ?? '0'),
			}
		)

	const liveRunListeners: Map<
		UniqueRunKey,
		Set<(runData: RunData) => void>
	> = new Map()

	coordinator.dataStore.eventEmitter.addListener('update', (key) => {
		const listeners = liveRunListeners.get(key)
		if (listeners) {
			const [runId, index] = getRunFromKey(key)
			const runData = coordinator.dataStore.getRunDataById(runId, index)
			if (!runData) {
				console.warn(
					`Tried to emit updates for run ${runId} but it has no data!`
				)
				return
			}
			for (const listener of listeners) {
				try {
					listener(runData)
				} catch (err) {
					console.error(
						`Failed to send run data for run ${runId} to subscribed client...`
					)
				}
			}
		}
	})

	const liveRunSummaryListeners: Set<(runData: RunSummariesData) => void> =
		new Set()

	coordinator.dataStore.eventEmitter.addListener('updateSummaries', () => {
		const summaries = coordinator.dataStore.getRunSummaries()
		for (const listener of liveRunSummaryListeners) {
			try {
				listener(summaries)
			} catch (err) {
				console.error(`Failed to send run summaries to subscribed client...`)
			}
		}
	})

	const liveMiningPoolListeners: Set<() => void> = new Set()
	miningPool.dataStore.eventEmitter.addListener('update', () => {
		for (const listener of liveMiningPoolListeners) {
			try {
				listener()
			} catch (err) {
				console.error(
					`Failed to send data for mining pool to subscribed client...`
				)
			}
		}
	})

	const fastify = Fastify({
		logger: true,
	})

	const shutdown = async () => {
		console.log('got shutdown signal, shutting down!')
		cancel()

		try {
			await fastify.close()
		} catch (err) {
			console.error('Error closing fastify:', err)
		}

		const shutdownTimeout = setTimeout(() => {
			console.error('Shutdown timeout reached, forcing exit!')
			process.exit(1)
		}, 10000)

		try {
			await Promise.all([coordinator.stopped, miningPool.stopped])
			clearTimeout(shutdownTimeout)
			console.log('Clean shutdown completed')
			process.exit(0)
		} catch (err) {
			console.error('Error during shutdown:', err)
			clearTimeout(shutdownTimeout)
			process.exit(1)
		}
	}

	let coordinatorCrashed: Error | null = null
	coordinator.stopped.catch((err) => {
		console.error(`[${Date.now()}] coordinator broken: `, err)
		coordinatorCrashed = new Error(err)
	})

	let miningPoolCrashed: Error | null = null
	miningPool.stopped.catch((err) => {
		console.error(`[${Date.now()}] mining pool broken: `, err)
		miningPoolCrashed = new Error(err)
	})

	process.on('SIGTERM', shutdown)

	process.on('SIGINT', shutdown)

	await fastify.register(cors, {
		origin: process.env.CORS_ALLOW_ORIGIN ?? true,
	})

	const initTime = Date.now()

	function getContributionInfo(
		req: FastifyRequest,
		res: Fastify.FastifyReply,
		address?: string
	) {
		const isStreamingRequest = req.headers.accept?.includes(
			'application/x-ndjson'
		)

		const data: ApiGetContributionInfo = {
			...miningPool.dataStore.getContributionInfo(address),
			miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
			error: miningPoolCrashed,
		}

		// set header for streaming/non
		res.header(
			'content-type',
			isStreamingRequest ? 'application/x-ndjson' : 'application/json'
		)

		if (!isStreamingRequest) {
			res.send(JSON.stringify(data, replacer))
			return
		}

		// start streaming newline-delimited json
		const stream = new PassThrough()
		res.send(stream)

		function sendContributionData() {
			const data: ApiGetContributionInfo = {
				...miningPool.dataStore.getContributionInfo(),
				miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
				error: miningPoolCrashed,
			}
			stream.write(JSON.stringify(data, replacer) + '\n')
		}

		// send the initial run data to populate the UI
		sendContributionData()

		// this listener will be called every time we see a state change.
		liveMiningPoolListeners.add(sendContributionData)

		// when the req closes, stop sending them updates
		req.socket.on('close', () => {
			liveMiningPoolListeners.delete(sendContributionData)
			stream.end()
		})
	}
	fastify.get('/contributionInfo', (req: FastifyRequest, res) => {
		getContributionInfo(req, res)
	})
	fastify.get(
		'/contributionInfo/:address',
		(req: FastifyRequest<{ Params: { address?: string } }>, res) => {
			getContributionInfo(req, res, req.params.address)
		}
	)

	fastify.get('/runs', (req, res) => {
		const isStreamingRequest = req.headers.accept?.includes(
			'application/x-ndjson'
		)

		const data: ApiGetRuns = {
			...coordinator.dataStore.getRunSummaries(),
			error: coordinatorCrashed,
		}

		// set header for streaming/non
		res.header(
			'content-type',
			isStreamingRequest ? 'application/x-ndjson' : 'application/json'
		)

		if (!isStreamingRequest) {
			res.send(JSON.stringify(data, replacer))
			return
		}

		// start streaming newline-delimited json
		const stream = new PassThrough()
		res.send(stream)

		function sendRunSummariesData(runSummariesData: RunSummariesData) {
			const data: ApiGetRuns = {
				...runSummariesData,
				error: coordinatorCrashed,
			}
			stream.write(JSON.stringify(data, replacer) + '\n')
		}

		// send the initial run summaries data to populate the UI
		sendRunSummariesData(data)

		// this listener will be called every time we see a state change.
		liveRunSummaryListeners.add(sendRunSummariesData)

		// when the req closes, stop sending them updates
		req.socket.on('close', () => {
			liveRunSummaryListeners.delete(sendRunSummariesData)
			stream.end()
		})
	})

	fastify.get(
		'/run/:runId/:indexStr',
		(
			req: FastifyRequest<{
				Params: { runId?: string; indexStr?: string }
			}>,
			res
		) => {
			const isStreamingRequest = req.headers.accept?.includes(
				'application/x-ndjson'
			)
			const { runId, indexStr } = req.params

			const index = Number.parseInt(indexStr ?? '0')
			if (`${index}` !== indexStr) {
				throw new Error(`Invalid index ${indexStr}`)
			}

			const matchingRun = runId
				? coordinator.dataStore.getRunDataById(runId, index)
				: null

			const data: ApiGetRun = {
				run: matchingRun,
				error: coordinatorCrashed,
				isOnlyRun: coordinator.dataStore.getNumRuns() === 1,
			}

			// set header for streaming/non
			res.header(
				'content-type',
				isStreamingRequest ? 'application/x-ndjson' : 'application/json'
			)

			if (!isStreamingRequest || !matchingRun) {
				res.send(JSON.stringify(data, replacer))
				return
			}

			const key = runKey(matchingRun.info.id, matchingRun.info.index)
			let listeners = liveRunListeners.get(key)
			if (!listeners) {
				listeners = new Set()
				liveRunListeners.set(key, listeners)
			}

			// start streaming newline-delimited json
			const stream = new PassThrough()
			res.send(stream)

			function sendRunData(runData: RunData) {
				const data: ApiGetRun = {
					run: runData,
					error: coordinatorCrashed,
					isOnlyRun: coordinator.dataStore.getNumRuns() === 1,
				}
				stream.write(JSON.stringify(data, replacer) + '\n')
			}

			// send the initial run data to populate the UI
			sendRunData(matchingRun)

			// this listener will be called every time we see a state change.
			listeners.add(sendRunData)

			// when the req closes, stop sending them updates
			req.socket.on('close', () => {
				listeners.delete(sendRunData)
				stream.end()
			})
		}
	)

	fastify.get<{
		Querystring: { owner: string; repo: string; revision?: string }
	}>('/check-checkpoint', async (request) => {
		const { owner, repo, revision } = request.query
		const url = `https://huggingface.co/${owner}/${repo}${revision ? `/tree/${revision}` : ''}`
		try {
			const response = await fetch(url, { method: 'HEAD' })
			return { isValid: response.ok, description: response.statusText }
		} catch (error) {
			const errorMessage =
				error instanceof Error ? error.message : 'Unknown error'
			return { isValid: false, description: errorMessage }
		}
	})

	fastify.get<{
		Querystring: { bucket: string; prefix?: string }
	}>('/check-gcs-bucket', async (request) => {
		const { bucket, prefix } = request.query
		const path = prefix ? `${prefix}/` : ''
		const url = `https://storage.googleapis.com/${bucket}/${path}manifest.json`
		try {
			const response = await fetch(url, { method: 'HEAD' })
			return { isValid: response.ok, description: response.statusText }
		} catch (error) {
			const errorMessage =
				error instanceof Error ? error.message : 'Unknown error'
			return { isValid: false, description: errorMessage }
		}
	})

	fastify.get('/status', async (_, res) => {
		const data = {
			commit: process.env.GITCOMMIT ?? '???',
			initTime,
			coordinator: {
				status: coordinatorCrashed ? coordinatorCrashed.toString() : 'ok',
				errors: coordinator.errors,
				trackedRuns: coordinator.dataStore.getRunSummaries().runs.map((r) => ({
					id: r.id,
					index: r.index,
					status: r.status,
				})),
				chain: {
					chainSlotHeight: await coordinatorRpc.getSlot('confirmed'),
					indexedSlot:
						coordinator.dataStore.lastUpdate().highestSignature?.slot ?? 0,
					programId:
						process.env.COORDINATOR_PROGRAM_ID ?? coordinatorIdl.address,
					networkGenesis: await coordinatorRpc.getGenesisHash(),
				},
			},
			miningPool: {
				status: miningPoolCrashed ? miningPoolCrashed.toString() : 'ok',
				errors: miningPool.errors,
				chain: {
					chainSlotHeight: await miningPoolRpc.getSlot('confirmed'),
					indexedSlot:
						miningPool.dataStore.lastUpdate().highestSignature?.slot ?? 0,
					programId:
						process.env.MINING_POOL_PROGRAM_ID ?? miningPoolIdl.address,
					networkGenesis: await miningPoolRpc.getGenesisHash(),
				},
			},
		} satisfies IndexerStatus
		res
			.header('content-type', 'application/json')
			.send(JSON.stringify(data, replacer))
	})

	await fastify.listen({ host: '0.0.0.0', port: 3000 })
}
main()
