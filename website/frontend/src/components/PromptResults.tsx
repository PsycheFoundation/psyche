import { useEffect, useState } from 'react'
import { detokenize } from '../utils/tokenizer.js'
import { loadPromptTextByIndex, getPromptName } from '../utils/prompts.js'
import { c } from '../utils.js'
import { css } from '@linaria/core'
import { text } from '../fonts.js'

interface PromptResultsProps {
	tokens: number[]
	promptIndex?: number
}

export function PromptResults({ tokens, promptIndex }: PromptResultsProps) {
	const [detokenizedText, setDetokenizedText] = useState<string>('')
	const [promptText, setPromptText] = useState<string>('')
	const [promptName, setPromptName] = useState<string>('')
	const [isLoading, setIsLoading] = useState(true)
	const [showTokens, setShowTokens] = useState(false)

	// Load prompt text when promptIndex changes
	useEffect(() => {
		if (promptIndex !== undefined) {
			loadPromptTextByIndex(promptIndex)
				.then((text) => {
					setPromptText(text)
				})
				.catch((error) => {
					console.error('Failed to load prompt text:', error)
					setPromptText('[Failed to load prompt]')
				})

			getPromptName(promptIndex)
				.then((name) => {
					setPromptName(name)
				})
				.catch(() => {
					setPromptName(`Prompt ${promptIndex}`)
				})
		} else {
			setPromptText('')
			setPromptName('')
		}
	}, [promptIndex])

	// Detokenize results when tokens change
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

	if (tokens.length === 0 && promptIndex === undefined) {
		return (
			<div
				className={c(
					css`
						padding: 16px;
						border-radius: 8px;
						max-width: 800px;
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
					Latest Prompt & Results:
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
					max-width: 800px;
				`,
				text['body/base/regular']
			)}
		>
			<div
				className={c(
					text['body/base/medium'],
					css`
						margin-bottom: 16px;
					`
				)}
			>
				Latest Prompt & Results:
				{tokens.length > 0 && (
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
				)}
			</div>

			{/* Show Prompt Text */}
			{promptText && (
				<div
					className={css`
						margin-bottom: 16px;
					`}
				>
					<div
						className={c(
							text['body/small/medium'],
							css`
								margin-bottom: 8px;
								color: #666;
							`
						)}
					>
						{promptName ? `${promptName}:` : 'Prompt:'}
					</div>
					<div
						className={css`
							border: 1px solid #e9ecef;
							border-radius: 4px;
							padding: 12px;
							font-family: 'Georgia', serif;
							font-size: 14px;
							line-height: 1.5;
							white-space: pre-wrap;
							color: #495057;
						`}
					>
						{promptText}
					</div>
				</div>
			)}

			{/* Show Results */}
			{tokens.length > 0 && (
				<div>
					<div
						className={c(
							text['body/small/medium'],
							css`
								margin-bottom: 8px;
								color: #666;
							`
						)}
					>
						Generated Response:
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
								padding: 12px;
								border-radius: 4px;
								border: 1px solid #e9ecef;
								display: block;
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
								border-left: 4px solid #007bff;
								white-space: pre-wrap;
							`}
						>
							"{detokenizedText}"
						</div>
					)}
				</div>
			)}

			{tokens.length === 0 && promptIndex !== undefined && (
				<div
					className={css`
						font-style: italic;
						color: #666;
						text-align: center;
						padding: 12px;
					`}
				>
					(generating response...)
				</div>
			)}
		</div>
	)
}
