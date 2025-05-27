import { styled } from '@linaria/react'
import { useEffect, useMemo, useState } from 'react'
import { CurrencyInput } from 'headless-currency-input'
import { css } from '@linaria/core'
import { c, formatUSDollars } from '../utils.js'
import { text } from '../fonts.js'
import { Button } from './Button.js'
import {
	findLender,
	getMiningPoolPDA,
	miningPoolIdl,
	PsycheSolanaMiningPool,
} from 'shared'
import { useWalletMultiButton } from '@solana/wallet-adapter-base-ui'
import ArrowLeft from '../assets/icons/arrow-left.svg?react'
import Smiley from '../assets/icons/smiley.svg?react'
import { error, forest } from '../colors.js'
import { WalletAddress } from './WalletAddress.js'
import { BN, Program } from '@coral-xyz/anchor'

import {
	getAssociatedTokenAddressSync,
	getAccount as getTokenAccount,
} from '@solana/spl-token'
import {
	useAnchorWallet,
	useConnection,
	useWallet,
} from '@solana/wallet-adapter-react'
import { PublicKey, Transaction } from '@solana/web3.js'
const Wrapper = styled.div`
	height: 256px;
	display: flex;
	justify-content: space-between;
	flex-direction: column;
	align-items: center;
	gap: 16px;
`

const currencyInput = css`
	border: none;
	width: 100%;
	background: transparent;
	text-align: center;
	flex-grow: 1;
	color: ${forest[600]};
`

const Balance = styled.span`
	text-transform: uppercase;
	color: ${forest[400]};
	&.poor {
		color: ${error[400]};
	}
`

const SideAlign = styled.span`
	width: 100%;
	display: flex;
	justify-content: space-between;
`

const DiscordLink = styled.a`
	color: ${forest[400]};
`

const PSYCHE_POOL_INDEX = 0n

export function GiveMoney({
	onExit,
	remainingMoney,
	miningPoolProgramId,
	collateralMintAddress,
	collateralMintDecimals,
}: {
	onExit: () => void
	remainingMoney: bigint
	collateralMintAddress: string
	miningPoolProgramId: string
	collateralMintDecimals: number
}) {
	const { connection } = useConnection()
	const { onDisconnect, publicKey } = useWalletMultiButton({
		onSelectWallet() {},
	})

	const { sendTransaction } = useWallet()
	const [collateralInfo, setCollateralInfo] = useState<{
		userCollateralAmount: bigint
		associatedTokenAddress: PublicKey
	} | null>(null)
	const [txErr, setTxErr] = useState<any | null>(null)
	const [sending, setSending] = useState<
		| { type: false }
		| { type: 'sending'; amount: BigInt }
		| { type: 'sent'; amount: BigInt }
	>({ type: false })
	const wallet = useAnchorWallet()
	const program = useMemo(
		() =>
			new Program<PsycheSolanaMiningPool>(
				{ ...miningPoolIdl, address: miningPoolProgramId } as any,
				{ connection, wallet } as any
			),
		[miningPoolProgramId]
	)

	const psychePoolPda = useMemo(
		() =>
			getMiningPoolPDA(new PublicKey(miningPoolProgramId), PSYCHE_POOL_INDEX),
		[miningPoolProgramId]
	)

	useEffect(() => {
		;(async () => {
			const mintAddr = new PublicKey(collateralMintAddress)
			console.log(
				'fetching associated token addr for collateral mint address',
				{ collateralMintAddress }
			)
			const myWalletCollateral = getAssociatedTokenAddressSync(
				mintAddr,
				publicKey!
			)

			// if there's no token account for the user, they have no USDC.
			const account = await getTokenAccount(
				connection,
				myWalletCollateral,
				'confirmed'
			).catch((err) => {
				console.warn(
					`Failed to fetch collateral address ${myWalletCollateral}, assuming user has no USDC`,
					err
				)
				return {
					amount: 0n,
				}
			})

			console.log('fetched myWalletCollateral account', {
				myWalletCollateral,
				account,
			})

			// const mint = await getMint(connection, mintAddr)
			setCollateralInfo({
				associatedTokenAddress: myWalletCollateral,
				userCollateralAmount: account.amount,
			})
		})()
	}, [collateralMintAddress, publicKey])
	const [money, setMoney] = useState('0.00')

	useEffect(() => {
		if (!publicKey) {
			onExit()
		}
	}, [])

	if (!publicKey) {
		// we're navigating back instantly on disconnect,
		// so it doesn't matter if we render nothing.
		return <></>
	}

	const fundingUnitsPerDollar = 10 ** collateralMintDecimals

	const walletBalance = collateralInfo?.userCollateralAmount ?? 0n
	const walletAddress = publicKey?.toString()
	const maxAmount =
		walletBalance > remainingMoney ? remainingMoney : walletBalance
	// have to div by 100 because we replace x.xx with xxx, so 100x too big.
	const contributeAmount =
		(BigInt(money.replace('.', '')) * BigInt(fundingUnitsPerDollar)) / 100n

	const poolFull = remainingMoney === 0n

	return (
		<>
			<div>
				<Button
					style="action"
					icon={{
						side: 'left',
						svg: ArrowLeft,
					}}
					onClick={onExit}
				>
					back
				</Button>
			</div>
			<Wrapper>
				{sending.type === 'sent' ? (
					<>
						<div className={c(currencyInput, text['display/5xl'])}>
							thank you!
						</div>
						<Balance className={text['body/sm/medium']}>
							you have provided $
							{(Number(sending.amount) / fundingUnitsPerDollar).toFixed(2)} to
							the pool
						</Balance>
					</>
				) : poolFull ? (
					<>
						<div className={c(currencyInput, text['display/5xl'])}>
							the pool is full
						</div>
						<Balance className={text['body/sm/medium']}>
							<DiscordLink href="https://discord.com/invite/jqVphNsB4H">
								join our discord
							</DiscordLink>{' '}
							for future updates about psyche
						</Balance>
					</>
				) : (
					<>
						<CurrencyInput
							autoFocus
							disabled={sending.type !== false}
							className={c(currencyInput, text['display/5xl'])}
							onValueChange={(values) => {
								if (values.value !== 'NaN') {
									setMoney(values.value)
								}
							}}
							currency="USD"
							locale="en-US"
						/>
						<Balance
							className={c(
								text['body/sm/medium'],
								contributeAmount > walletBalance ? 'poor' : ''
							)}
						>
							{'wallet balance '}
							{formatUSDollars(
								Number(maxAmount) / Number(fundingUnitsPerDollar)
							)}{' '}
							{'USDC'}
						</Balance>
						{txErr && (
							<Balance className={c(text['body/sm/medium'], 'poor')}>
								{txErr.toString()}
							</Balance>
						)}
					</>
				)}

				<SideAlign>
					<span className={c(text['body/sm/medium'], walletAddress)}>
						<WalletAddress>{walletAddress}</WalletAddress>
						<Button style="secondary" onClick={onDisconnect}>
							x
						</Button>
					</span>
					<Button
						disabled={
							contributeAmount > maxAmount ||
							contributeAmount === 0n ||
							sending.type !== false ||
							!collateralInfo
						}
						style="primary"
						icon={{ side: 'left', svg: Smiley }}
						onClick={async () => {
							if (!collateralInfo) {
								console.warn('button should be disabled, no collateralInfo')
								return
							}
							setSending({ type: 'sending', amount: contributeAmount })
							try {
								const tx = new Transaction()

								const lenderPda = findLender(
									new PublicKey(miningPoolProgramId),
									psychePoolPda,
									publicKey
								)
								const lenderAccount =
									await program.account.lender.fetchNullable(lenderPda)

								if (!lenderAccount) {
									tx.add(
										await program.methods
											.lenderCreate({})
											.accounts({
												pool: psychePoolPda,
												payer: publicKey,
												user: publicKey,
												// // @ts-expect-error anchor doesn't think this is part of the type. but it is. hover it. it's there. wtf.
												// lender: lenderPda,
											})
											.instruction()
									)
								}
								tx.add(
									await program.methods
										.lenderDeposit({
											collateralAmount: new BN(contributeAmount.toString()),
										})
										.accounts({
											pool: psychePoolPda,
											userCollateral: collateralInfo.associatedTokenAddress,
											user: publicKey,
										})
										.instruction()
								)

								await sendTransaction(tx, connection)
								setSending({ type: 'sent', amount: contributeAmount })
							} catch (err) {
								console.error(err)
								setTxErr(err)
								setSending({ type: false })
							}
						}}
					>
						{!collateralInfo
							? 'loading...'
							: sending.type === false
								? 'contribute compute'
								: sending.type === 'sending'
									? 'sending contribution...'
									: 'contribution sent!'}
					</Button>
				</SideAlign>
				<span className={text['aux/xs/regular']}>
					Any capital contributed to this pool is purely a donation and for
					testing purposes only. Any digital tokens made available by Nous or
					the Psyche Foundation on the Testnet, any tokens configured using the
					Testnet, and any tokens configured using any extrinsics available for
					the Testnet have no economic or monetary value and cannot be exchanged
					for or converted into cash, cash equivalent, or value.
				</span>
			</Wrapper>
		</>
	)
}
