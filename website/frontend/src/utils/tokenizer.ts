// Simple LLama2 tokenizer implementation
let tokenizer: { vocab_reverse: Record<number, string> } | null = null

export async function loadTokenizer() {
	if (tokenizer) return tokenizer

	try {
		const response = await fetch('/llama2_tokenizer.json')
		const tokenizerData = await response.json()

		// Extract vocabulary from tokenizer.json
		const vocabReverse: Record<number, string> = {}

		// Process added_tokens (special tokens like <unk>, <s>, </s>)
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

		tokenizer = { vocab_reverse: vocabReverse }
		console.log(
			`Loaded tokenizer with ${Object.keys(vocabReverse).length} tokens`
		)
		return tokenizer
	} catch (error) {
		console.error('Failed to load tokenizer:', error)
		return null
	}
}

export async function detokenize(tokenIds: number[]): Promise<string> {
	const tok = await loadTokenizer()
	if (!tok) {
		// Fallback: just show token IDs
		return tokenIds.map((id) => `<${id}>`).join('')
	}

	// Convert token IDs to raw pieces
	const pieces: string[] = []
	for (const tokenId of tokenIds) {
		const tokenText = tok.vocab_reverse[tokenId]
		if (tokenText !== undefined) {
			pieces.push(tokenText)
		} else {
			pieces.push(`<UNK_${tokenId}>`) // Unknown token
		}
	}

	// Join pieces and process SentencePiece markers
	let text = pieces.join('')

	// Handle SentencePiece ▁ markers (these represent word boundaries/spaces)
	text = text.replace(/▁/g, ' ')

	// Handle hex byte tokens (like <0x0A> for newline)
	text = text.replace(/<0x([0-9A-Fa-f]{2})>/g, (match, hex) => {
		const byte = parseInt(hex, 16)
		if (byte === 0x0a) return '\n'
		if (byte === 0x09) return '\t'
		if (byte === 0x0d) return '\r'
		if (byte >= 0x20 && byte <= 0x7e) return String.fromCharCode(byte) // printable ASCII
		return match
	})

	// Handle special tokens as to not show them in final text
	text = text
		.replace(/<s>/g, '')
		.replace(/<\/s>/g, '')
		.replace(/<unk>/g, '[UNK]')

	// Clean up only excessive spaces (but preserve intentional newlines/tabs)
	text = text.replace(/ +/g, ' ').trim()

	return text || '[Empty]'
}
