import { forest, lime, slate } from '../colors.js'
import { text } from '../fonts.js'
import { Button } from './Button.js'
import { styled } from '@linaria/react'
import { ContributionInfo } from 'shared'
import MedusaHead from '../assets/icons/medusa-head.svg?react'
import { useCallback, useEffect, useState } from 'react'
import { GiveMoney } from './GiveMoney.js'
import Smiley from '../assets/icons/smiley.svg?react'
import { useWalletMultiButton } from '@solana/wallet-adapter-base-ui'
import { useWalletModal } from '@solana/wallet-adapter-react-ui'
import { useWallet } from '@solana/wallet-adapter-react'
import { WalletReadyState } from '@solana/wallet-adapter-base'
import { createSphereAnimation } from '../gl/regl.js'
import { useDarkMode } from 'usehooks-ts'
import { iconClass } from '../icon.js'
import { c, formatUSDollars } from '../utils.js'
import { Address } from './Address.js'
const TopBox = styled.div`
	padding: 0 16px;
	display: flex;
	flex-direction: column;
	gap: 24px;
	flex-shrink: 0;
	position: relative;
`

const ContributePoolLine = styled.div`
	display: flex;
	justify-content: space-between;
	.theme-light & {
		color: ${forest[500]};
	}
	.theme-dark & {
		color: ${forest[300]};
	}
`

const ContributeProgress = styled.span`
	.theme-light & {
		color: ${slate[1000]};
	}
	.theme-dark & {
		color: ${forest[300]};
	}
`

const ProgressBarBg = styled.div`
	width: ${(props) => props.widthCh}ch;
	position: relative;
	.theme-light & {
		background: ${slate[300]};
		color: ${forest[500]};
	}
	.theme-dark & {
		background: ${forest[600]};
		color: ${lime[300]};
	}
`

const ProgressBarFilled = styled.span`
	position: absolute;
	width: ${(props) => props.widthCh}ch;
	height: 100%;
	background-image:
		radial-gradient(currentColor 0.6px, transparent 0),
		radial-gradient(currentColor 0.6px, transparent 0);
	background-size:
		2.5px 2.5px,
		2.5px 2.5px;
	background-position:
		2px 2px,
		3.25px 3.25px;
`

const ProgressBarUnfilled = styled.div`
	width: ${(props) => props.widthCh}ch;
	position: relative;
	text-align: center;
	& > span {
		padding: 0 2px;
		margin: 0 auto;
		.theme-light & {
			background: ${slate[300]};
		}
		.theme-dark & {
			background: ${forest[600]};
		}
	}
`

function ContributeProgressBar({
	ratio,
	widthCh,
}: {
	ratio: number
	widthCh: number
}) {
	return (
		<ProgressBarBg widthCh={widthCh} className={text['aux/sm/regular']}>
			<ProgressBarFilled
				widthCh={Number.isNaN(ratio) ? 1 : ratio * widthCh}
				bgColor={slate[300]}
			/>
			<ProgressBarUnfilled widthCh={widthCh}>
				<span>
					{Number.isNaN(ratio) ? '100%' : `${(ratio * 100).toFixed(4)}%`}
				</span>
			</ProgressBarUnfilled>
		</ProgressBarBg>
	)
}

const ProgressContainer = styled.div`
	display: flex;
	flex-direction: row;
	align-items: center;
	gap: 16px;
`

const RankTable = styled.table`
	position: absolute;
	top: 0;
	left: 0;
	right: 0;
	bottom: 0;
	width: 100%;
	margin-top: 16px;
	border-collapse: collapse;

	& th {
		text-align: left;
		text-transform: uppercase;
		position: sticky;
		top: 0;
		z-index: 1;
		.theme-light & {
			background: ${slate[500]};
			outline-color: ${slate[500]};
			color: ${slate[0]};
		}
		.theme-dark & {
			background: ${forest[600]};
			outline-color: ${forest[600]};
			color: ${forest[300]};
		}
		outline: 1px solid;
		outline-top: 4px solid;
		outline-bottom: 4px solid;
	}

	& tbody tr {
		border-top: 1px solid;
		border-bottom: 1px solid;

		.theme-light & {
			color: ${slate[600]};
			border-color: ${slate[300]};
			&.featured {
				color: ${forest[500]};
				border-color: ${forest[400]};
				.icon {
					color: ${forest[400]};
				}
			}
		}
		.theme-dark & {
			color: ${forest[300]};
			border-color: ${forest[600]};

			&.featured {
				color: ${lime[400]};
				border-color: ${forest[400]};
				.icon {
					color: ${lime[400]};
				}
			}
		}

		&.featured td:first-child {
			text-align: center;
		}
		&.isme {
			.theme-dark & {
				background: ${lime[400]};
				color: ${slate[1000]};
				&.featured .icon {
					color: ${slate[1000]};
				}
			}
			.theme-light & {
				background: ${forest[200]};
			}
		}
	}

	& td {
		padding: 3px 8px;
		a {
			color: inherit;
			text-decoration: none;
		}
	}
	& td & .featured td:nth-child(1) {
		display: flex;
		align-items: center;
	}

	& th {
		padding: 0 8px;
	}
`

const OrbCanvas = styled.canvas`
	width: 256px;
	height: 256px;
	margin: 0 auto;
	image-rendering: crisp-edges;
	image-rendering: pixelated;
`

const TableContainer = styled.div`
	position: relative;
	height: 100%;
	overflow-y: auto;
	min-height: 256px;
`
const AddressBox = styled.td`
	width: 100%;
	max-width: 0;
	overflow: hidden;
	white-space: nowrap;
`
const Loading = styled.div`
	text-align: center;
	padding: 2lh;
`

export function ContributeCompute({
	contributionInfo,
}: {
	contributionInfo: ContributionInfo | null
}) {
	const [walletConnectTriggered, setWalletConnectTriggered] = useState(false)
	const [contributing, setContributing] = useState(false)
	const { wallets, select } = useWallet()

	const { setVisible: setModalVisible } = useWalletModal()
	const { buttonState, onConnect } = useWalletMultiButton({
		onSelectWallet() {
			setModalVisible(true)
		},
	})

	// if we only have one wallet, pick it!
	useEffect(() => {
		const installed = wallets.filter(
			(w) => w.readyState === WalletReadyState.Installed
		)
		if (installed.length === 1 && wallets.length === 1) {
			select(installed[0].adapter.name)
		}
	}, [wallets])
	useEffect(() => {
		if (buttonState === 'has-wallet' && walletConnectTriggered && onConnect) {
			onConnect()
		}
	}, [buttonState, onConnect])

	const connectWalletOrContribute = useCallback(() => {
		setContributing(true)
		if (buttonState === 'no-wallet') {
			setModalVisible(true)
			setWalletConnectTriggered(true)
		} else if (buttonState === 'has-wallet') {
			if (onConnect) {
				onConnect()
			}
		}
	}, [buttonState, onConnect])

	const { publicKey } = useWallet()

	const [canvas, setCanvas] = useState<null | HTMLCanvasElement>(null)
	const canvasRef = useCallback((node: HTMLCanvasElement) => {
		setCanvas(node)
	}, [])
	const { isDarkMode } = useDarkMode()

	useEffect(() => {
		if (!canvas || (contributing && buttonState === 'connected')) {
			return
		}
		const color = isDarkMode ? forest[300] : forest[500]
		return createSphereAnimation(canvas, color)
	}, [canvas, isDarkMode, contributing, buttonState])

	const canvasSize = 256

	if (!contributionInfo) {
		return <Loading className={text['body/base/regular']}>Loading...</Loading>
	}
	const {
		totalDepositedCollateralAmount,
		maxDepositCollateralAmount,
		users,
		collateralMintAddress,
		miningPoolProgramId,
		collateralMintDecimals,
	} = contributionInfo

	return (
		<>
			<TopBox>
				<ProgressContainer>
					<ContributeProgress className={text['body/base/medium']}>
						POOL CAPACITY
					</ContributeProgress>
					<ContributeProgressBar
						ratio={
							Number(totalDepositedCollateralAmount) /
							Number(maxDepositCollateralAmount)
						}
						widthCh={18}
					/>
				</ProgressContainer>
				{contributing && buttonState === 'connected' ? (
					<GiveMoney
						onExit={() => setContributing(false)}
						remainingMoney={
							maxDepositCollateralAmount - totalDepositedCollateralAmount
						}
						collateralMintAddress={collateralMintAddress}
						miningPoolProgramId={miningPoolProgramId}
						collateralMintDecimals={collateralMintDecimals}
					/>
				) : (
					<>
						<OrbCanvas ref={canvasRef} width={canvasSize} height={canvasSize} />
						<ContributePoolLine>
							<span className={text['body/base/medium']}>
								MINING POOL:{' '}
								{formatUSDollars(
									Number(totalDepositedCollateralAmount) /
										10 ** collateralMintDecimals
								)}
							</span>
							<Button
								disabled={buttonState === 'connecting'}
								style="primary"
								onClick={connectWalletOrContribute}
								icon={{ side: 'left', svg: Smiley }}
							>
								{buttonState === 'connecting'
									? 'connecting...'
									: buttonState !== 'connected'
										? 'connect wallet'
										: 'donate'}
							</Button>
						</ContributePoolLine>
					</>
				)}
			</TopBox>
			<TableContainer>
				<RankTable>
					<thead className={text['body/sm/semibold']}>
						<tr>
							<th>Rank</th>
							<th>Address</th>
							<th>Contribution</th>
						</tr>
					</thead>
					<tbody className={text['button/sm']}>
						{users.map((user, i) => {
							const featured = i < 3
							const fundingPercent =
								Number(
									(user.funding * 100_000n) / totalDepositedCollateralAmount
								) / 1_000
							return (
								<tr
									key={user.address}
									className={c(
										featured && 'featured',
										publicKey?.toString() === user.address && 'isme'
									)}
								>
									<td>
										{featured ? (
											<MedusaHead className={iconClass} />
										) : (
											user.rank.toString().padStart(2, '0')
										)}
									</td>
									<AddressBox className={featured ? text['display/2xl'] : ''}>
										<Address
											address={user.address}
											cluster={import.meta.env.VITE_MINING_POOL_CLUSTER}
										/>
									</AddressBox>
									<td className={featured ? text['body/xl/medium'] : ''}>
										{fundingPercent < 0.001
											? '<0.001'
											: +fundingPercent.toFixed(3)}
										%
									</td>
								</tr>
							)
						})}
						{Array.from({ length: Math.max(0, 7 - users.length) }, (_, i) => (
							<tr key={`fake-${i}`}>
								<td />
								<td>&nbsp;</td>
								<td />
							</tr>
						))}
					</tbody>
				</RankTable>
			</TableContainer>
		</>
	)
}
