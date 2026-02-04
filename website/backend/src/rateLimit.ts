export function createRateLimitedPromise<
	T extends (...args: any[]) => Promise<any>,
>(promiseFn: T, maxCalls: number, timeWindowMs: number): T {
	interface QueuedRequest {
		args: Parameters<typeof promiseFn>
		resolve: (value: Awaited<ReturnType<T>>) => void
		reject: (reason?: any) => void
	}

	const queue: QueuedRequest[] = []
	const completionTimes: number[] = []
	let inFlightCount = 0

	const processQueue = (): void => {
		// drop timestamps older than our rate limit window
		const now = Date.now()
		while (completionTimes.length && completionTimes[0] <= now - timeWindowMs) {
			completionTimes.shift()
		}

		// fire every request we have space for in our queue
		while (queue.length && completionTimes.length + inFlightCount < maxCalls) {
			const request = queue.shift()!
			inFlightCount++

			Promise.resolve(promiseFn(...request.args))
				.then((result) => request.resolve(result))
				.catch((error) => request.reject(error))
				.finally(() => {
					inFlightCount--
					completionTimes.push(Date.now())

					// if we're the first one to complete,
					// schedule processing the queue again after the time window
					if (queue.length && completionTimes.length === 1) {
						setTimeout(processQueue, timeWindowMs)
					}
				})
		}

		// check the queue on the next expiration
		if (queue.length && completionTimes.length) {
			const nextProcessTime = completionTimes[0] + timeWindowMs - now
			setTimeout(processQueue, nextProcessTime)
		}
	}

	return function rateLimitedPromise(...args: Parameters<typeof promiseFn>) {
		return new Promise((resolve, reject) => {
			queue.push({
				args,
				resolve,
				reject,
			})

			processQueue()
		})
	} as any
}

/**
 * Creates a function that wraps a promise-returning function with retry logic
 * @param fn The function to wrap with retry logic
 * @param options Configuration options for the retry behavior
 * @returns A wrapped function that includes retry logic
 */
export function makeRetryPromise<T extends (...args: any[]) => Promise<any>>(
	fn: T,
	options: {
		maxRetries?: number
		retryInitTimeMs?: number
		retryMult?: number
		onRetry?: (attempt: number, delay: number, error?: unknown) => void
	} = {}
): T {
	const {
		maxRetries = 10,
		retryInitTimeMs = 1000,
		retryMult = 2.5,
		onRetry = (attempt, delay) =>
			console.warn(`retrying attempt ${attempt} after ${delay}ms...`),
	} = options

	return (async (...args: Parameters<T>): Promise<ReturnType<T>> => {
		let attempts = 0

		while (true) {
			attempts += 1
			let error
			try {
				return await fn(...args)
			} catch (err) {
				console.warn(err)
				if (attempts >= maxRetries) {
					throw err
				}
				error = err
			}

			const retryDelay = retryInitTimeMs * retryMult ** (attempts - 1)
			onRetry(attempts, retryDelay, error)
			await new Promise((resolve) => setTimeout(resolve, retryDelay))
		}
	}) as T
}

export function makeRateLimitedFetch(): typeof fetch {
	const MAX_RETRIES = 10
	const RETRY_INIT_TIME_MS = 1000
	const RETRY_MULT = 2.5
	const REQS_PER_SECOND = 200
	const rateLimitedFetch = createRateLimitedPromise(
		fetch,
		REQS_PER_SECOND,
		1000
	)

	return makeRetryPromise(
		async (...args: Parameters<typeof fetch>): ReturnType<typeof fetch> => {
			const result = await rateLimitedFetch(...args)

			if (!result.ok) {
				const body = await result.text()
				throw new Error(
					`fetch of ${args[0]} failed. status: ${result.status} - ${body}`
				)
			}

			return result
		},
		{
			maxRetries: MAX_RETRIES,
			retryInitTimeMs: RETRY_INIT_TIME_MS,
			retryMult: RETRY_MULT,
			onRetry: (attempt, delay) =>
				console.warn(
					`fetch failed on attempt ${attempt}. retrying after ${delay}...`
				),
		}
	)
}
