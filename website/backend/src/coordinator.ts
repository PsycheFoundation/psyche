export type UniqueRunKey = `${string}-${string}-${string}` & {
	__uniqueRunKey: true
}

export function runKey(
	programId: string,
	runId: string,
	index: number
): UniqueRunKey {
	return `${programId}-${runId}-${index}` as UniqueRunKey
}

export function getRunFromKey(
	runKey: UniqueRunKey
): [programId: string, runId: string, index: number] {
	const parts = runKey.split('-')
	if (parts.length < 3) {
		// Backward compatibility: runId-index
		const [runId, index] = splitAtLastInstance(runKey, '-')
		return ['', runId, Number.parseInt(index, 10)]
	}

	// programId-runId-index
	// Handle case where runId might contain dashes by joining middle parts
	const programId = parts[0]
	const index = parts[parts.length - 1]
	const runId = parts.slice(1, -1).join('-')

	return [programId, runId, Number.parseInt(index, 10)]
}

function splitAtLastInstance(text: string, splitAt: string): [string, string] {
	var index = text.lastIndexOf(splitAt)
	return [text.slice(0, index), text.slice(index + 1)]
}
