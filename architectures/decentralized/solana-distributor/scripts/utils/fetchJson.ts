import { ErrorStack, JsonValue } from 'solana-kiss'

export async function fetchJson(
	url: string,
	method: string,
	body?: JsonValue
): Promise<JsonValue> {
	const response = await fetch(url, {
		method: method,
		headers: { 'Content-Type': 'application/json' },
		body: body ? JSON.stringify(body) : undefined,
	})
	if (!response.ok) {
		throw new ErrorStack(
			`Failed to fetch JSON from ${url}`,
			await response.text()
		)
	}
	return response.json()
}
