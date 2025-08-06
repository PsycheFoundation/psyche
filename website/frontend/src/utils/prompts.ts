// Utility for loading prompt text by index from the frontend
interface PromptInfo {
	index: number
	name: string
	file?: string // Optional - for file-based prompts
	text?: string // Optional - for inline text prompts
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
	// Initialize cache if needed
	if (!promptsCache) {
		promptsCache = new Map()
	}

	// Return from cache if available
	if (promptsCache.has(index)) {
		return promptsCache.get(index)!
	}

	try {
		// Load prompts index
		const promptIndex = await loadPromptsIndex()

		// Find the prompt info for this index
		const promptInfo = promptIndex.prompts.find((p) => p.index === index)
		if (!promptInfo) {
			throw new Error(`Prompt index ${index} not found`)
		}

		let text: string

		// Handle inline text prompts
		if (promptInfo.text) {
			text = promptInfo.text
		}
		// Handle file-based prompts
		else if (promptInfo.file) {
			const response = await fetch(`/prompts/${promptInfo.file}`)
			if (!response.ok) {
				throw new Error(`Failed to fetch prompt file: ${promptInfo.file}`)
			}
			text = await response.text()
		}
		// Error if neither text nor file is provided
		else {
			throw new Error(`Prompt ${index} has neither text nor file specified`)
		}

		// Cache the result
		promptsCache.set(index, text)

		return text
	} catch (error) {
		console.error(`Failed to load prompt text for index ${index}:`, error)
		// Return a fallback
		return `[Error loading prompt ${index}]`
	}
}

export async function getPromptName(index: number): Promise<string> {
	try {
		const promptIndex = await loadPromptsIndex()
		const promptInfo = promptIndex.prompts.find((p) => p.index === index)
		return promptInfo?.name || `Prompt ${index}`
	} catch (error) {
		return `Prompt ${index}`
	}
}
