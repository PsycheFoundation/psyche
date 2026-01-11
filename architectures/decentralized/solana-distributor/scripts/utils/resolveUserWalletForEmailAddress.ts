import { PrivyClient } from '@privy-io/node'
import { resolveUserWallet } from './resolveUserWallet'

export async function resolveUserWalletForEmailAddress(
	privyClient: PrivyClient,
	emailAddress: string
) {
	return await resolveUserWallet(
		privyClient,
		emailAddress,
		() => privyClient.users().getByEmailAddress({ address: emailAddress }),
		async () => {
			return privyClient.users().create({
				linked_accounts: [{ type: 'email', address: emailAddress }],
			})
		}
	)
}
