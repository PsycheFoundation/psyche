// Kept some of the logic for llama2, in case we want to switch back, although we use llama3 by default

let tokenizer: {
	vocab_reverse: Record<number, string>
	model_type: 'llama2' | 'llama3'
} | null = null

async function loadTokenizerForModel(modelType: 'llama2' | 'llama3') {
	const tokenizerFile =
		modelType === 'llama3'
			? '/tokenizers/llama3_tokenizer.json'
			: '/tokenizers/llama2_tokenizer.json'

	try {
		const response = await fetch(tokenizerFile)
		const tokenizerData = await response.json()

		// Extract vocabulary from tokenizer.json
		const vocabReverse: Record<number, string> = {}

		// Process added_tokens (special tokens)
		if (tokenizerData.added_tokens) {
			for (const token of tokenizerData.added_tokens) {
				vocabReverse[token.id] = token.content
			}
		}

		// Process main vocabulary
		if (tokenizerData.model && tokenizerData.model.vocab) {
			for (const [content, id] of Object.entries(tokenizerData.model.vocab)) {
				vocabReverse[id as number] = content
			}
		}

		return { vocab_reverse: vocabReverse, model_type: modelType }
	} catch (error) {
		console.error(`Failed to load ${modelType} tokenizer:`, error)
		return null
	}
}

export async function loadTokenizer() {
	if (tokenizer) return tokenizer

	const modelType = 'llama3'
	let result = await loadTokenizerForModel(modelType)

	if (result) {
		tokenizer = result
		console.log(
			`Loaded ${result.model_type} tokenizer with ${Object.keys(result.vocab_reverse).length} tokens`
		)
	}

	return tokenizer
}

export async function detokenize(tokenIds: number[]): Promise<string> {
	const tok = await loadTokenizer()
	if (!tok) {
		// Fallback: just show token IDs
		return tokenIds.map((id) => `<${id}>`).join('')
	}

	// Filter out padding and problematic tokens before processing
	const cleanedTokenIds = tokenIds.filter(
		(id) => id !== 0 && id !== null && id !== undefined && !Number.isNaN(id)
	)

	// Convert token IDs to raw pieces
	const pieces: string[] = []
	for (const tokenId of cleanedTokenIds) {
		const tokenText = tok.vocab_reverse[tokenId]
		if (tokenText !== undefined) {
			pieces.push(tokenText)
		} else {
			pieces.push(`<UNK_${tokenId}>`) // Unknown token
		}
	}

	// Join pieces and process SentencePiece markers
	let text = pieces.join('')

	// Handle SentencePiece markers
	text = text
		.replace(/▁/g, ' ') // standard SentencePiece space marker
		.replace(/Ġ/g, ' ') // LLaMA 3 space marker
		.replace(/Ċ/g, '\n') // LLaMA 3 newline marker
		.replace(/âĢĺ/g, '"') // Left quote marker
		.replace(/âĢĻ/g, '"') // Right quote marker
		.replace(/âĢĵ/g, "'") // Apostrophe marker
		.replace(/âĢĶ/g, '-') // Dash marker
		.replace(/âĢī/g, '...') // Ellipsis marker

	// Handle hex byte tokens (like <0x0A> for newline)
	text = text.replace(/<0x([0-9A-Fa-f]{2})>/g, (match, hex) => {
		const byte = parseInt(hex, 16)
		if (byte === 0x0a) return '\n'
		if (byte === 0x09) return '\t'
		if (byte === 0x0d) return '\r'
		if (byte >= 0x20 && byte <= 0x7e) return String.fromCharCode(byte) // printable ASCII
		return match
	})

	// Handle more special tokens
	text = text
		.replace(/<\|begin_of_text\|>/g, '')
		.replace(/<\|end_of_text\|>/g, '')
		.replace(/<\|start_header_id\|>/g, '')
		.replace(/<\|end_header_id\|>/g, '')
		.replace(/<\|eot_id\|>/g, '')
		.replace(/<\|reserved_special_token_\d+\|>/g, '')
		.replace(/<unk>/g, '[UNK]')
		.replace(/<s>/g, '')
		.replace(/<\/s>/g, '')
		.replace(/<unk>/g, '[UNK]')

	// Clean up only excessive spaces (but preserve intentional newlines/tabs)
	text = text.replace(/ +/g, ' ').trim()

	return text || '[Empty]'
}
