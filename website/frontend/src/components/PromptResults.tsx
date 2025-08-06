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
	const [previousTokensLength, setPreviousTokensLength] = useState(0)
	const [newTokensHighlight, setNewTokensHighlight] = useState(false)

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

	// Detokenize results when tokens change and detect new tokens
	useEffect(() => {
		if (tokens.length === 0) {
			setDetokenizedText('')
			setIsLoading(false)
			setPreviousTokensLength(0)
			return
		}

		// Detect if new tokens were added
		const hasNewTokens = tokens.length > previousTokensLength
		if (hasNewTokens) {
			setNewTokensHighlight(true)
			// Remove highlight after animation
			setTimeout(() => setNewTokensHighlight(false), 100)
		}
		setPreviousTokensLength(tokens.length)

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
	}, [tokens, previousTokensLength])

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
					<>
						<span
							className={c(
								css`
									margin-left: 8px;
									font-size: 12px;
									padding: 2px 6px;
									border-radius: 12px;
									color: #666;
									background-color: #f8f9fa;
									border: 1px solid #dee2e6;
									transition: all 0.3s ease;
								`,
								newTokensHighlight &&
									css`
										background-color: #e8f5e8 !important;
										border: 1px solid #28a745 !important;
									`
							)}
						>
							{tokens.length} tokens
							{newTokensHighlight && (
								<span
									className={css`
										color: #28a745;
										font-weight: bold;
										margin-left: 4px;
									`}
								>
									+{tokens.length - previousTokensLength}
								</span>
							)}
						</span>
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
					</>
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
							text['body/sm/medium'],
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
							text['body/sm/medium'],
							css`
								margin-bottom: 8px;
								color: #666;
							`
						)}
					>
						Generated Response:
					</div>
					{isLoading ? (
						<div
							className={css`
								padding: 12px;
								border-radius: 4px;
								border-left: 4px solid #007bff;
								background: #f8f9fa;
								display: flex;
								align-items: center;
								gap: 8px;
							`}
						>
							<div
								className={css`
									width: 12px;
									height: 12px;
									border: 2px solid #007bff;
									border-top: 2px solid transparent;
									border-radius: 50%;
									animation: spin 1s linear infinite;
									@keyframes spin {
										0% {
											transform: rotate(0deg);
										}
										100% {
											transform: rotate(360deg);
										}
									}
								`}
							/>
							<span
								className={css`
									font-style: italic;
									color: #666;
								`}
							>
								Detokenizing...
							</span>
						</div>
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
							className={c(
								css`
									font-family: 'Georgia', serif;
									font-size: 16px;
									line-height: 1.6;
									padding: 12px;
									border-radius: 4px;
									border-left: 4px solid #007bff;
									white-space: pre-wrap;
									background-color: transparent;
									transition: all 0.3s ease;
									position: relative;
									max-height: 200px;
									overflow-y: auto;
									border: 1px solid #e9ecef;
								`,
								newTokensHighlight &&
									css`
										border-left: 4px solid #28a745 !important;
										background-color: #f8fff8 !important;
									`
							)}
						>
							"{detokenizedText}"
							{newTokensHighlight && (
								<span
									className={css`
										position: absolute;
										top: -8px;
										right: 8px;
										background: #28a745;
										color: white;
										font-size: 11px;
										padding: 2px 6px;
										border-radius: 12px;
										font-weight: bold;
									`}
								>
									NEW
								</span>
							)}
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
