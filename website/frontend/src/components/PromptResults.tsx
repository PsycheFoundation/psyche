import { useEffect, useState } from 'react'
import { detokenize } from '../utils/tokenizer.js'
import { loadPromptTextByIndex } from '../utils/prompts.js'
import { c } from '../utils.js'
import { css } from '@linaria/core'
import { text } from '../fonts.js'
import { forest, slate } from '../colors.js'

interface PromptResultsProps {
	tokens: number[]
	promptIndex?: number
}

export function PromptResults({ tokens, promptIndex }: PromptResultsProps) {
	const [detokenizedText, setDetokenizedText] = useState<string>('')
	const [promptText, setPromptText] = useState<string>('')
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
		} else {
			setPromptText('')
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
			setTimeout(() => setNewTokensHighlight(false), 250)
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
		return <></>
	}

	return (
		<div
			className={c(
				css`
					width: 600px;
				`,
				text['body/base/regular']
			)}
		>
			{/* Show Prompt Text with token info */}
			{promptText && (
				<div
					className={css`
						margin-bottom: 12px;
						display: flex;
						align-items: center;
						gap: 16px;
					`}
				>
					<div
						className={css`
							font-family: 'Georgia', serif;
							font-size: 16px;
							line-height: 1.5;
							font-weight: 500;
							flex: 1;

							.theme-light & {
								color: ${slate[1000]};
							}

							.theme-dark & {
								color: ${forest[300]};
							}
						`}
						title={promptText}
					>
						{promptText.length > 80
							? promptText.substring(0, 80) + '...'
							: promptText}
					</div>
					{tokens.length > 0 && (
						<div
							className={css`
								display: flex;
								align-items: center;
								gap: 8px;
								font-size: 14px;

								.theme-light & {
									color: ${forest[600]};
									background-color: ${slate[200]};
								}

								.theme-dark & {
									color: ${forest[400]};
									background-color: ${forest[700]};
								}
							`}
						>
							<span>({tokens.length} tokens)</span>
							<button
								onClick={() => setShowTokens(!showTokens)}
								className={css`
									background: none;
									border: none;
									cursor: pointer;
									font-size: 14px;
									text-decoration: underline;

									.theme-light & {
										color: ${forest[600]};
										&:hover {
											color: ${forest[500]};
										}
									}

									.theme-dark & {
										color: ${forest[400]};
										&:hover {
											color: ${forest[300]};
										}
									}
								`}
							>
								[show {showTokens ? 'text' : 'tokens'}]
							</button>
						</div>
					)}
				</div>
			)}

			{/* Show Results in box */}
			<div
				className={c(
					css`
						border: 1px solid #4b9551;
						border-radius: 8px;
						padding: 16px;
						height: 200px;
					`,
					newTokensHighlight &&
						css`
							.theme-light & {
								border-color: ${forest[500]} !important;
								background-color: ${slate[100]} !important;
							}

							.theme-dark & {
								border-color: ${forest[400]} !important;
								background-color: #29442aff !important;
							}
						`
				)}
			>
				{/* Content */}
				{tokens.length === 0 && promptIndex !== undefined ? (
					<div
						className={css`
							font-style: italic;
							text-align: center;
							height: 100%;
							display: flex;
							align-items: center;
							justify-content: center;

							.theme-light & {
								color: ${slate[600]};
							}

							.theme-dark & {
								color: ${forest[400]};
							}
						`}
					>
						(generating response...)
					</div>
				) : isLoading ? (
					<div
						className={css`
							display: flex;
							align-items: center;
							justify-content: center;
							gap: 8px;
							height: 100%;
						`}
					>
						<div
							className={css`
								width: 12px;
								height: 12px;
								border: 2px solid;
								border-top: 2px solid transparent;
								border-radius: 50%;
								animation: spin 1s linear infinite;

								.theme-light & {
									border-color: ${slate[600]};
									border-top-color: transparent;
								}

								.theme-dark & {
									border-color: ${forest[400]};
									border-top-color: transparent;
								}

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

								.theme-light & {
									color: ${slate[600]};
								}

								.theme-dark & {
									color: ${forest[400]};
								}
							`}
						>
							Detokenizing...
						</span>
					</div>
				) : showTokens ? (
					<div
						className={css`
							font-family: 'Courier New', monospace;
							font-size: 12px;
							word-break: break-all;
							line-height: 1.4;
							height: 100%;
							overflow-y: auto;

							.theme-light & {
								color: ${slate[700]};
							}

							.theme-dark & {
								color: ${forest[400]};
							}
						`}
					>
						[{tokens.join(', ')}]
					</div>
				) : (
					<div
						className={css`
							font-family: 'Georgia', serif;
							font-size: 15px;
							line-height: 1.6;
							white-space: pre-wrap;
							height: 100%;
							overflow-y: auto;

							.theme-light & {
								color: ${slate[600]};
							}

							.theme-dark & {
								color: ${forest[400]};
							}
						`}
					>
						{detokenizedText}
					</div>
				)}
			</div>
		</div>
	)
}
