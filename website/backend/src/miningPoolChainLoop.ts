import { Program } from '@coral-xyz/anchor'
import { getMiningPoolPDA, PsycheSolanaMiningPool } from 'shared'
import { PsycheMiningPoolInstructionsUnion } from './idlTypes.js'
import { MiningPoolDataStore } from './dataStore.js'
import { startWatchChainLoop } from './chainLoop.js'
import { getMint } from '@solana/spl-token'

export async function startWatchMiningPoolChainLoop(
	dataStore: MiningPoolDataStore,
	miningPool: Program<PsycheSolanaMiningPool>,
	websocketRpcUrl: string,
	cancelled: { cancelled: boolean }
) {
	const ourPool = getMiningPoolPDA(miningPool.programId, 0n)
	await startWatchChainLoop<PsycheMiningPoolInstructionsUnion>()(
		'mining pool',
		dataStore,
		miningPool,
		websocketRpcUrl,
		cancelled,
		{
			onStartCatchup(firstStateEver) {
				return {
					userAccountsUpdated: new Set<`${string}:${string}`>(),
					mainAccountUpdated: firstStateEver,
				}
			},
			onInstruction(_tx, instruction, decoded, state) {
				switch (decoded.name) {
					case 'lender_deposit': {
						const user = instruction.accounts[0]
						const lender = instruction.accounts[4]
						state.userAccountsUpdated.add(`${user}:${lender}`)
						state.mainAccountUpdated = true
						break
					}
					case 'pool_extract':
					case 'pool_update': {
						state.mainAccountUpdated = true
						break
					}
				}
			},
			async onDoneCatchup(store, state) {
				if (state.mainAccountUpdated) {
					const account = await miningPool.account.pool.fetch(ourPool)
					store.setFundingData(account)
					if (!store.hasCollateralInfo()) {
						const { decimals } = await getMint(
							miningPool.provider.connection,
							account.collateralMint
						)
						store.setCollateralInfo(account.collateralMint.toString(), decimals)
					}
				}
				const updatedAddresses = [...state.userAccountsUpdated.values()].map(
					(s) => s.split(':') as [string, string]
				)
				for (const [user, lenderAccountAddress] of updatedAddresses) {
					const account =
						await miningPool.account.lender.fetch(lenderAccountAddress)
					if (!account) {
						console.warn(
							`[mining pool] failed to fetch account for lender ${lenderAccountAddress} mentioned in tx data...`
						)
						continue
					}
					store.setUserAmount(
						user,
						BigInt(account.depositedCollateralAmount.toString())
					)
				}
			},
		},
		5_000
	)
}
