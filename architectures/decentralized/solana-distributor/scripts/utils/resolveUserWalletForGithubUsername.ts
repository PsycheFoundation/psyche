import { PrivyClient } from '@privy-io/node'
import { resolveUserWallet } from './resolveUserWallet'
import { fetchJson } from './fetchJson'
import { jsonAsNumber, jsonGetAt, ErrorStack } from 'solana-kiss'

export async function resolveUserWalletForGithubUsername(
	privyClient: PrivyClient,
	githubUsername: string
) {
	return await resolveUserWallet(
		privyClient,
		githubUsername,
		() => privyClient.users().getByGitHubUsername({ username: githubUsername }),
		async () => {
			const githubUserInfo = await fetchJson(
				`https://api.github.com/users/${encodeURIComponent(githubUsername)}`,
				'GET'
			)
			const githubUserId = jsonAsNumber(jsonGetAt(githubUserInfo, 'id'))
			if (!githubUserId) {
				throw new ErrorStack(
					'Failed to fetch GitHub user info: ' + githubUserInfo
				)
			}
			return privyClient.users().create({
				linked_accounts: [
					{
						type: 'github_oauth',
						subject: githubUserId.toString(),
						username: githubUsername,
					},
				],
			})
		}
	)
}
