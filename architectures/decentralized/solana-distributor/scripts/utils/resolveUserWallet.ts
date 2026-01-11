import { PrivyClient, User } from '@privy-io/node'
import { ErrorStack, Pubkey, pubkeyFromBase58 } from 'solana-kiss'

export async function resolveUserWallet<P>(
	privyClient: PrivyClient,
	userKey: P,
	userFetcher: () => Promise<User>,
	userFactory: () => Promise<User>
) {
	try {
		return getOrCreateUserWallet(privyClient, await userFetcher())
	} catch (fetchError) {
		try {
			return getOrCreateUserWallet(privyClient, await userFactory())
		} catch (createError) {
			throw new ErrorStack(
				'Failed to resolve or create user: ' + String(userKey),
				[fetchError, createError]
			)
		}
	}
}

async function getOrCreateUserWallet(
	privyClient: PrivyClient,
	existingUser: User
) {
	const existingWallet = getUserSolanaWalletAddress(existingUser)
	if (existingWallet) {
		return { privyUser: existingUser, walletAddress: existingWallet }
	}
	const updatedUser = await privyClient
		.users()
		.pregenerateWallets(existingUser.id, {
			wallets: [{ chain_type: 'solana' }],
		})
	const updatedWallet = getUserSolanaWalletAddress(updatedUser)
	if (updatedWallet) {
		return { privyUser: updatedUser, walletAddress: updatedWallet }
	}
	throw new ErrorStack(
		'Failed to create a solana pregenerated wallet for user id: ' +
			existingUser.id
	)
}

function getUserSolanaWalletAddress(privyUser: User): Pubkey | null {
	for (const linkedAccount of privyUser.linked_accounts) {
		if (linkedAccount.type !== 'wallet') {
			continue
		}
		if (linkedAccount.chain_type !== 'solana') {
			continue
		}
		return pubkeyFromBase58(linkedAccount.address)
	}
	return null
}
