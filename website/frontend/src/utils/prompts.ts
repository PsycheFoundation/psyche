// Utility for loading prompt text by the frontend
// Use only either of these options
interface PromptInfo {
	file?: string // for file-based prompts (e.g. macbeth.txt)
	text?: string // inline text prompts
}

interface PromptsIndex {
	prompts: PromptInfo[]
}

let promptsCache: Map<number, string> | null = null
let promptsIndex: PromptsIndex | null = null

export async function loadPromptsIndex(): Promise<PromptsIndex> {
	if (promptsIndex) return promptsIndex

	try {
		const response = await fetch('/prompts/index.json')
		promptsIndex = await response.json()
		return promptsIndex!
	} catch (error) {
		console.error('Failed to load prompts index:', error)
		throw error
	}
}

export async function loadPromptTextByIndex(index: number): Promise<string> {
	if (!promptsCache) {
		promptsCache = new Map()
	}
	if (promptsCache.has(index)) {
		return promptsCache.get(index)!
	}

	try {
		// Load prompts index
		const promptIndex = await loadPromptsIndex()
		const promptInfo = promptIndex.prompts[index]
		if (!promptInfo) {
			throw new Error(`Prompt index ${index} not found`)
		}

		let text: string
		if (promptInfo.text) {
			text = promptInfo.text
		} else if (promptInfo.file) {
			const response = await fetch(`/prompts/${promptInfo.file}`)
			if (!response.ok) {
				throw new Error(`Failed to fetch prompt file: ${promptInfo.file}`)
			}
			text = await response.text()
		} else {
			throw new Error(`Prompt ${index} has neither text nor file specified`)
		}

		promptsCache.set(index, text)
		return text
	} catch (error) {
		console.error(`Failed to load prompt text for index ${index}:`, error)
		return `[Error loading prompt ${index}]`
	}
}
