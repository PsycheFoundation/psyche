import {
	IndexerStatus,
	ApiGetRuns,
	ApiGetRun,
	ApiGetContributionInfo,
	ApiGetCheckpointStatus,
	formats,
	CURRENT_VERSION,
} from 'shared'
import {
	fakeContributionInfo,
	fakeIndexerStatus,
	makeFakeRunData,
	fakeRunSummaries,
} from './fakeData.js'

const { protocol, hostname } = window.location
const port = import.meta.env.VITE_BACKEND_PORT ?? window.location.port
const origin = `${protocol}//${hostname}${port ? `:${port}` : ''}`

let path = import.meta.env.VITE_BACKEND_PATH ?? '/'
path = path.startsWith('/') ? path.substring(1) : path

if (path && !path.startsWith('/')) {
	path = `/${path}`
}

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL ?? `${origin}${path}`

function psycheJsonFetch(path: string) {
	return fetch(`${BACKEND_URL}/${path}`)
		.then(async (r) => [r, await r.text()] as const)
		.then(([r, text]) => {
			if (r.status !== 200) {
				throw new Error(`Failed to fetch ${path}: ${text}`)
			}
			return text
		})
		.then((text) => JSON.parse(text, formats[CURRENT_VERSION].reviver))
}

export async function fetchStatus(): Promise<IndexerStatus> {
	return import.meta.env.VITE_FAKE_DATA
		? fakeIndexerStatus
		: psycheJsonFetch('status')
}

export async function fetchRuns(): Promise<ApiGetRuns> {
	return import.meta.env.VITE_FAKE_DATA
		? ({
				runs: fakeRunSummaries,
				totalTokens: 1_000_000_000n,
				totalTokensPerSecondActive: 23_135_234n,
			} satisfies ApiGetRuns)
		: psycheJsonFetch('runs')
}

export async function fetchCheckpointStatus(
	owner: string,
	repo: string,
	revision?: string
): Promise<ApiGetCheckpointStatus> {
	const queryParams = `owner=${owner}&repo=${repo}${revision ? `&revision=${revision}` : ''}`
	return psycheJsonFetch(`check-checkpoint?${queryParams}`)
}

export async function fetchGcsCheckpointStatus(
	bucket: string,
	prefix?: string
): Promise<ApiGetCheckpointStatus> {
	const queryParams = `bucket=${bucket}${prefix ? `&prefix=${prefix}` : ''}`
	return psycheJsonFetch(`check-gcs-bucket?${queryParams}`)
}

interface DecodeState {
	buffer: string
	decoder: TextDecoder
}

function makeDecodeState(): DecodeState {
	return {
		buffer: '',
		decoder: new TextDecoder('utf-8'),
	}
}

export async function fetchRunStreaming(
	runId: string,
	indexStr: string,
	programId: string
): Promise<ReadableStream<ApiGetRun>> {
	if (import.meta.env.VITE_FAKE_DATA) {
		const seed = Math.random() * 1_000_000_000
		try {
			const index = Number.parseInt(indexStr ?? '0')
			if (`${index}` !== indexStr) {
				throw new Error(`Invalid index ${indexStr}`)
			}
			return new ReadableStream<ApiGetRun>({
				async start(controller) {
					let i = 0
					while (true) {
						controller.enqueue({
							run: makeFakeRunData[runId](seed, i, index),
							isOnlyRun: false,
						})
						const nextFakeDataDelay = 1000 + Math.random() * 1000
						await new Promise((r) => setTimeout(r, nextFakeDataDelay))
						i++
					}
				},
			})
		} catch (err) {
			return new ReadableStream({
				async start(controller) {
					controller.close()
				},
			})
		}
	}

	console.log('opening run stream for', runId, indexStr, programId)
	return makeStreamingNdJsonDecode(`run/${runId}/${programId}/${indexStr}`)
}

export async function fetchSummariesStreaming(): Promise<
	ReadableStream<ApiGetRuns>
> {
	if (import.meta.env.VITE_FAKE_DATA) {
		return new ReadableStream<ApiGetRuns>({
			async start(controller) {
				let i = 0
				while (true) {
					controller.enqueue({
						runs: fakeRunSummaries,
						totalTokens: 1_000_000_000n + BigInt(i * 10000),
						totalTokensPerSecondActive: 23_135_234n,
					})
					const nextFakeDataDelay = 1000 + Math.random() * 1000
					await new Promise((r) => setTimeout(r, nextFakeDataDelay))
					i++
				}
			},
		})
	}

	console.log('opening summaries stream')

	return makeStreamingNdJsonDecode(`runs`)
}

export async function fetchContributionsStreaming(): Promise<
	ReadableStream<ApiGetContributionInfo>
> {
	if (import.meta.env.VITE_FAKE_DATA) {
		return new ReadableStream<ApiGetContributionInfo>({
			async start(controller) {
				let i = 0
				while (true) {
					controller.enqueue(fakeContributionInfo)
					const nextFakeDataDelay = 1000 + Math.random() * 1000
					await new Promise((r) => setTimeout(r, nextFakeDataDelay))
					i++
				}
			},
		})
	}

	console.log('opening contributions stream')

	return makeStreamingNdJsonDecode(`contributionInfo`)
}

async function makeStreamingNdJsonDecode<T>(backendPath: string) {
	let { reader, decodeState } = await openStreamToBackendPath(backendPath)
	let streamController: ReadableStreamDefaultController<T>
	let lastData: T | null = null
	let isCanceled = false
	const stream = new ReadableStream<T>({
		async start(controller) {
			streamController = controller
			const MAX_RECONNECT_ATTEMPTS = 5
			let reconnectAttempts = 0
			let reconnectDelay = 1000

			try {
				while (true) {
					const nextPayload = await getOneJsonPayloadFromStream<T>(
						decodeState,
						reader
					)
					if (nextPayload) {
						decodeState = nextPayload.decodeState
						lastData = nextPayload.parsedPayload
						// only enqueue if not canceled and controller is not closed
						if (!isCanceled) {
							try {
								controller.enqueue(nextPayload.parsedPayload)
							} catch (err) {
								console.log(
									'Failed to enqueue data (stream likely closed):',
									err
								)
								break
							}
						}
						continue
					}

					console.log('closing reader')

					await reader.cancel()

					// don't reconnect if the stream was canceled
					if (isCanceled) {
						console.log('Stream was canceled, not reconnecting')
						break
					}

					// we failed to fetch the next json data because the stream ended - let's reconnect
					if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
						console.log(
							`Stream ended, attempting to reconnect (${reconnectAttempts + 1}/${MAX_RECONNECT_ATTEMPTS})...`
						)
						reconnectAttempts++
						await new Promise((resolve) => setTimeout(resolve, reconnectDelay))
						reconnectDelay = Math.min(reconnectDelay * 2, 10000)

						try {
							const newStream = await openStreamToBackendPath(backendPath)
							reader = newStream.reader
							decodeState = newStream.decodeState

							// if we opened a new stream successfully, we're good to go. reset our reconnect attempts
							reconnectAttempts = 0
							reconnectDelay = 1000
							continue // and start reading from the new connection
						} catch (reconnectError) {
							console.error('Failed to reconnect:', reconnectError)
							throw reconnectError
						}
					} else {
						console.log(
							'Maximum reconnection attempts reached, closing stream.'
						)
						break
					}
				}
			} catch (error) {
				console.error('Stream processing error:', error)
				controller.error(error)
			} finally {
				controller.close()
			}
		},
		cancel(reason) {
			console.log(`Canceling stream for ${backendPath}`, reason)
			isCanceled = true
			reader.cancel(reason)
		},
	})

	// When a new reader attaches, make sure they get the current data :)
	const getReader = stream.getReader.bind(stream)
	stream.getReader = ((...args: Parameters<typeof getReader>) => {
		const reader = getReader(...args)

		if (lastData && !isCanceled) {
			try {
				streamController.enqueue(lastData)
			} catch (err) {
				console.log('Failed to enqueue lastData (stream likely closed):', err)
			}
		}

		return reader
	}) as typeof getReader
	return stream
}

async function getOneJsonPayloadFromStream<T>(
	decodeState: DecodeState,
	reader: ReadableStreamDefaultReader<Uint8Array<ArrayBufferLike>>
) {
	let parsedPayload: T | null = null
	while (!parsedPayload) {
		const { value, done } = await reader.read()

		if (done) {
			return null
		}

		decodeState.buffer += decodeState.decoder.decode(value, {
			stream: true,
		})
		const lines = decodeState.buffer.split('\n')

		if (lines.length > 1) {
			// we have at least one complete line, so one full JSON object
			const firstLine = lines[0].trim()
			if (firstLine) {
				parsedPayload = JSON.parse(
					firstLine,
					formats[CURRENT_VERSION].reviver
				) as T
				decodeState.buffer = lines.slice(1).join('\n')
			} else {
				decodeState.buffer = lines.slice(1).join('\n')
			}
		}
	}
	return { parsedPayload, decodeState }
}

async function openStreamToBackendPath(path: string) {
	const response = await fetch(`${BACKEND_URL}/${path}`, {
		headers: {
			Accept: 'application/x-ndjson',
		},
	})

	if (!response.ok || !response.body) {
		throw new Error('Failed to fetch run data')
	}
	if (
		!(response.headers.get('Content-Type') ?? 'missing').includes(
			'application/x-ndjson'
		)
	) {
		throw new Error(
			`Invalid content type on response: expected "application/x-ndjson", got "${response.headers.get('Content-Type')}"`
		)
	}

	return {
		reader: response.body.getReader(),
		decodeState: makeDecodeState(),
	}
}
