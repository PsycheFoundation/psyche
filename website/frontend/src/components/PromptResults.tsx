import { useEffect, useState } from 'react'
import { detokenize } from '../utils/tokenizer.js'
import { c } from '../utils.js'
import { css } from '@linaria/core'
import { text } from '../fonts.js'

interface PromptResultsProps {
	tokens: number[]
}

export function PromptResults({ tokens }: PromptResultsProps) {
	const [detokenizedText, setDetokenizedText] = useState<string>('')
	const [isLoading, setIsLoading] = useState(true)
	const [showTokens, setShowTokens] = useState(false)

	useEffect(() => {
		if (tokens.length === 0) {
			setDetokenizedText('')
			setIsLoading(false)
			return
		}

		setIsLoading(true)
		detokenize(tokens)
			.then((text) => {
				setDetokenizedText(text)
				setIsLoading(false)
			})
			.catch((error) => {
				console.error('Failed to detokenize:', error)
				setDetokenizedText(`[Failed to detokenize: ${tokens.join(', ')}]`)
				setIsLoading(false)
			})
	}, [tokens])

	if (tokens.length === 0) {
		return (
			<div
				className={c(
					css`
						padding: 16px;
						border-radius: 8px;
						max-width: 600px;
					`,
					text['body/base/regular']
				)}
			>
				<div
					className={c(
						text['body/base/medium'],
						css`
							margin-bottom: 8px;
						`
					)}
				>
					Latest Prompt Results:
				</div>
				<span
					className={css`
						font-style: italic;
						color: #666;
					`}
				>
					(no prompt results yet)
				</span>
			</div>
		)
	}

	return (
		<div
			className={c(
				css`
					padding: 16px;
					border-radius: 8px;
					max-width: 600px;
				`,
				text['body/base/regular']
			)}
		>
			<div
				className={c(
					text['body/base/medium'],
					css`
						margin-bottom: 8px;
					`
				)}
			>
				Latest Prompt Results:
				<button
					onClick={() => setShowTokens(!showTokens)}
					className={css`
						margin-left: 8px;
						background: none;
						border: 1px solid #ccc;
						border-radius: 4px;
						padding: 2px 8px;
						cursor: pointer;
						font-size: 12px;
						&:hover {
							background: #f0f0f0;
						}
					`}
				>
					{showTokens ? 'Show Text' : 'Show Tokens'}
				</button>
			</div>

			{isLoading ? (
				<span
					className={css`
						font-style: italic;
						color: #666;
					`}
				>
					Detokenizing...
				</span>
			) : showTokens ? (
				<span
					className={css`
						font-family: 'Courier New', monospace;
						font-size: 14px;
						word-break: break-all;
						line-height: 1.4;
					`}
				>
					[{tokens.join(', ')}]
				</span>
			) : (
				<div
					className={css`
						font-family: 'Georgia', serif;
						font-size: 16px;
						line-height: 1.6;
						padding: 12px;
						border-radius: 4px;
						border-left: 4px solid #0add7bff;
					`}
				>
					"{detokenizedText}"
				</div>
			)}
		</div>
	)
}
