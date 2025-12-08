import { PublicKey } from '@solana/web3.js'
import {
	ContributionInfo,
	IndexerStatus,
	RunData,
	RunRoundClient,
	RunState,
	RunSummary,
} from 'shared'

export const fakeIndexerStatus: IndexerStatus = {
	initTime: Date.now() - 1000 * 60 * 60 * 24 * 7, // 1 week ago
	commit: 'fake data',
	coordinator: {
		status: 'ok',
		chain: {
			chainSlotHeight: 123456,
			indexedSlot: 123450,
			programId: '0x1234567890abcdef1234567890abcdef12345678',
			networkGenesis:
				'0xdeadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef',
		},
		trackedRuns: [
			{ id: 'run-001', index: 0, status: { type: 'active' } },
			{ id: 'run-002', index: 0, status: { type: 'funding' } },
			{
				id: 'run-003',
				index: 0,
				status: {
					type: 'completed',
					at: {
						slot: 12345n,
						time: new Date(Date.now() - 1000 * 60 * 60 * 24),
					},
				},
			},
		],
		errors: [],
	},
	miningPool: {
		status: 'ok',
		chain: {
			chainSlotHeight: 123456,
			indexedSlot: 123450,
			programId: '0x9876543210fedcba9876543210fedcba98765432',
			networkGenesis:
				'0xdeadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef',
		},
		errors: [],
	},
}

export const fakeRunSummaries: RunSummary[] = [
	{
		id: 'run-001',
		index: 0,
		isOnlyRunAtThisIndex: false,
		name: 'Vision Model Alpha',
		description: 'Training a vision model to recognize everyday objects',
		status: { type: 'paused' },
		totalTokens: 100000n,
		size: 1000000000n,
		arch: 'HfLlama',
		type: 'vision',
		pauseHistory: [],
		lastUpdate: {
			time: new Date('2025-01-15T09:30:00'),
			slot: 123440n,
		},
	},
	{
		id: 'run-002',
		index: 0,
		isOnlyRunAtThisIndex: true,
		name: 'Text Assistant Beta',
		description: 'Assistant-like model with reasoning capabilities',
		status: { type: 'active' },
		totalTokens: 200000n,
		size: 2000000000n,
		arch: 'HfLlama',
		type: 'text',
		pauseHistory: [],
		lastUpdate: {
			time: new Date('2025-01-15T09:30:00'),
			slot: 123440n,
		},
	},
	{
		id: 'run-003',
		index: 0,
		isOnlyRunAtThisIndex: true,
		name: 'Small Language Model',
		description: 'Compact text model for edge devices',
		status: {
			type: 'completed',
			at: {
				slot: 12345n,
				time: new Date(Date.now() - 1000 * 60 * 60 * 24),
			},
		}, // 1 day ago
		totalTokens: 50000n,
		size: 500000000n,
		arch: 'HfLlama',
		type: 'text',
		pauseHistory: [],
		lastUpdate: {
			time: new Date('2025-01-15T09:30:00'),
			slot: 123440n,
		},
	},
	{
		id: 'run-001',
		index: 1,
		isOnlyRunAtThisIndex: false,
		name: 'Vision Model Alpha',
		description: 'Training a vision model to recognize everyday objects',
		status: { type: 'active' },
		totalTokens: 100000n,
		size: 1000000000n,
		arch: 'HfLlama',
		type: 'vision',
		pauseHistory: [],
		lastUpdate: {
			time: new Date('2025-01-15T09:30:00'),
			slot: 123440n,
		},
	},
]

export const makeFakeRunData: Record<
	string,
	(seed?: number, step?: number, index?: number) => RunData
> = {
	'run-001': (seed, step, index) =>
		index === 1
			? makeFakeRunDataSeeded(seed, step, index)
			: {
					info: fakeRunSummaries[0],
					recentTxs: [],
					promptResults: [],
					promptIndex: 0,
					cumulativePromptResults: [],
					metrics: {
						summary: {
							loss: 0.0,
							bandwidth: 0.0,
							tokensPerSecond: 0.0,
							evals: {},
							lr: 0.004,
							promptResults: [],
							promptIndex: 0,
							cumulativePromptResults: [],
						},
						history: {
							loss: [],
							bandwidth: [],
							tokensPerSecond: [],
							evals: {},
							lr: [],
							promptResults: [],
							promptIndex: [],
							cumulativePromptResults: [],
						},
					},
				},
	'run-002': () => ({
		info: fakeRunSummaries[1],
		recentTxs: [],
		promptResults: [],
		promptIndex: 0,
		cumulativePromptResults: [],
		metrics: {
			summary: {
				loss: 0.0,
				bandwidth: 0.0,
				tokensPerSecond: 0.0,
				evals: {},
				lr: 0.004,
				promptResults: [],
				promptIndex: 0,
				cumulativePromptResults: [],
			},
			history: {
				loss: [],
				bandwidth: [],
				tokensPerSecond: [],
				evals: {},
				lr: [],
				promptResults: [],
				promptIndex: [],
				cumulativePromptResults: [],
			},
		},
	}),
	'run-003': () => ({
		info: fakeRunSummaries[2],
		recentTxs: [],
		promptResults: [],
		promptIndex: 0,
		cumulativePromptResults: [],
		metrics: {
			summary: {
				loss: 0.18,
				bandwidth: 156.3,
				tokensPerSecond: 320.4,
				evals: {
					accuracy: 0.91,
					precision: 0.89,
					recall: 0.9,
				},
				lr: 0.0005,
				promptResults: [],
				promptIndex: 0,
				cumulativePromptResults: [],
			},
			history: {
				loss: randomWalk(1, 1, 1, 1.1),
				bandwidth: randomWalk(1, 1_000_000),
				tokensPerSecond: randomWalk(1, 100_000),
				lr: [],
				evals: {
					accuracy: randomWalk(1, 1),
					precision: randomWalk(1, 1),
					recall: randomWalk(1, 1),
				},
				promptResults: [],
				promptIndex: [],
				cumulativePromptResults: [],
			},
		},
	}),
}

export const fakeContributionInfo: ContributionInfo = {
	collateralMintDecimals: 6,
	totalDepositedCollateralAmount: 2500000000000n,
	maxDepositCollateralAmount: 2500000000000n,
	users: [
		{
			rank: 1,
			address: 'abc1234567890abcdef1234567890abcdef123456',
			funding: 500000000000n,
		},
		{
			rank: 2,
			address: 'def9876543210fedcba9876543210fedcba987654',
			funding: 300000000000n,
		},
		{
			rank: 3,
			address: '123abcdef0123456789abcdef0123456789abcdef',
			funding: 250000000000n,
		},
		{
			rank: 4,
			address: '456fedcba9876543210fedcba9876543210fedcba',
			funding: 120000000000n,
		},
		{
			rank: 5,
			address: '789abcdef0123456789abcdef0123456789abcdef',
			funding: 80000000000n,
		},
		...Array.from({ length: 300 }, (_, i) => ({
			rank: i + 6,
			address: `789abcdef0123456789abcdef0123456789abcdef${i}`,
			funding: 80000000000n,
		})),
	],
	collateralMintAddress: '4BJXYkfvg37zEmBbsacZjeQDpTNx91KppxFJxRqrz48e',
	miningPoolProgramId: '4BJXYkfvg37zEmBbsacZjeQDpTNx91KppxFJxRqrz48e',
}

function makeFakeRunDataSeeded(seed = 1, step = 0, index = 0): RunData {
	const seededRandom = createSeededRandom(seed)

	const epochTime = 3600
	const minClients = Math.round(seededRandom() * 10) + 2
	const totalClients = minClients

	const roundsPerEpoch = Math.round(seededRandom() * 10) + 10
	const stepsPerEpoch = roundsPerEpoch + 2 + totalClients // +2 for warmup and cooldown, +n for num clients
	const epochStep = step % stepsPerEpoch

	const clients = Array.from({ length: totalClients }, (_, i) => {
		const basePubkey = PublicKey.findProgramAddressSync(
			[new Uint8Array([i, seed])],
			PublicKey.default
		)[0].toString()
		return {
			pubkey: basePubkey,
			witness: false,
		} as RunRoundClient
	})

	let phase: RunState = 'Uninitialized'
	let round = 0

	if (epochStep < totalClients) {
		phase = 'WaitingForMembers'

		clients.forEach((_, i) => {
			if (i >= epochStep) {
				clients.splice(i)
			}
		})
	} else if (epochStep === totalClients) {
		phase = 'Warmup'
		round = 0
	} else if (epochStep === stepsPerEpoch - 1) {
		phase = 'Cooldown'
		round = roundsPerEpoch - 1
	} else {
		// Training rounds - alternating between RoundTrain and RoundWitness
		round = epochStep - (1 + totalClients)
		const isTraining = epochStep % 2 === 0
		phase = isTraining ? 'RoundTrain' : 'RoundWitness'

		clients.forEach((client, i) => {
			const clientSeedRandom = createSeededRandom(seed + i + epochStep)
			const isWitness = clientSeedRandom() > 0.5
			if (isTraining) {
				const state = clientSeedRandom()
				client.witness = isWitness ? (state > 0.5 ? 'done' : 'waiting') : false
			} else {
				client.witness = isWitness ? 'done' : false
			}
		})
	}

	return {
		info: { ...fakeRunSummaries[3], index },
		recentTxs: Array.from({ length: 5 }, (_, i) => ({
			pubkey: PublicKey.default.toString(),
			method: 'tick',
			data: '{}',
			txHash: PublicKey.default.toString() + step + i,
			timestamp: {
				slot: BigInt(step + i),
				time: new Date(Date.now() - step * 3_000),
			},
		})),
		promptResults: [],
		promptIndex: 0,
		cumulativePromptResults: [],
		metrics: {
			summary: {
				loss: 0.32 + seededRandom() * 0.3,
				bandwidth: seededRandom() * 128_000_000,
				tokensPerSecond: seededRandom() * 128_000,
				lr: seededRandom() * 0.004,
				evals: {
					accuracy: 0.83,
					precision: 0.79,
					recall: 0.85,
					bananas: 0.85,
					sphereEval: 0.85,
				},
				promptResults: [],
				promptIndex: 0,
				cumulativePromptResults: [],
			},
			history: {
				loss: randomWalk(seed, 1, undefined, undefined, step),
				bandwidth: randomWalk(seed, 1000000, undefined, undefined, step),
				tokensPerSecond: randomWalk(seed, 100000, undefined, undefined, step),
				lr: randomWalk(seed, 0.001, undefined, undefined, step),
				evals: {
					accuracy: randomWalk(seed, 1, undefined, undefined, step),
					precision: randomWalk(seed, 1, undefined, undefined, step),
					recall: randomWalk(seed, 1, undefined, undefined, step),
				},
				promptResults: [],
				promptIndex: [],
				cumulativePromptResults: [],
			},
		},
		state: {
			phase,
			phaseStartTime: new Date(Date.now() - seededRandom() * 2_000),
			epochStartTime: new Date(Date.now() - seededRandom() * 10_000),
			round,
			clients,
			checkpoint: {
				repo_id: 'PsycheFoundation/Skibbler',
				revision: null,
			},
			config: {
				cooldownTime: 5_000,
				maxRoundTrainTime: 5_000,
				warmupTime: 3_000,
				roundWitnessTime: 2_000,
				minClients,
				epochTime,
				lrSchedule: {
					Cosine: {
						base_lr: 4.0e-4,
						warmup_steps: 500,
						warmup_init_lr: 0.0,
						total_steps: 25000,
						final_lr: 4.0e-5,
					},
				},
			},
		},
	}
}

function createSeededRandom(seed: number) {
	return function () {
		const x = Math.sin(seed++) * 10000
		return x - Math.floor(x)
	}
}

function randomWalk(
	seed: number,
	scale: number,
	start = 0,
	down = 0.9,
	numStepsSet?: number
): Array<readonly [number, number]> {
	const seededRandom = createSeededRandom(seed)
	const numSteps =
		numStepsSet ?? Math.floor(seededRandom() * (2000 - 1000 + 1)) + 1000

	const walk = [[0 as number, start] as const]

	let currentValue = start

	for (let i = 1; i <= numSteps; i++) {
		const movement = (seededRandom() * 2 - down) * 0.005

		currentValue += movement
		currentValue = Math.max(Math.min(currentValue, 1), 0)

		walk.push([i, currentValue * scale] as const)
	}

	return walk
}
