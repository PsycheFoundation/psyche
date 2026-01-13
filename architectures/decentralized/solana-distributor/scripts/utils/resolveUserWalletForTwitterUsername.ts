import { PrivyClient } from "@privy-io/node";
import { ErrorStack, jsonAsString, jsonGetAt } from "solana-kiss";
import { fetchJson } from "./fetchJson";
import { resolveUserWallet } from "./resolveUserWallet";

export async function resolveUserWalletForTwitterUsername(
  privyClient: PrivyClient,
  sourceTab: string,
  twitterUsername: string,
  twitterBearerToken: string,
) {
  return await resolveUserWallet(
    privyClient,
    `Twitter User: ${twitterUsername} (${sourceTab})`,
    () =>
      privyClient.users().getByTwitterUsername({ username: twitterUsername }),
    async () => {
      const twitterUserInfo = await fetchJson(
        `https://api.x.com/2/users/by/username/${twitterUsername}`,
        "GET",
        undefined,
        { Authorization: `Bearer ${twitterBearerToken}` },
      );
      const twitterUserId = jsonAsString(jsonGetAt(twitterUserInfo, "data.id"));
      const twitterUserKey = jsonAsString(
        jsonGetAt(twitterUserInfo, "data.name"),
      );
      if (!twitterUserId || !twitterUserKey) {
        throw new ErrorStack(
          "Failed to fetch GitHub user info: " + twitterUserInfo,
        );
      }
      return privyClient.users().create({
        linked_accounts: [
          {
            type: "twitter_oauth",
            subject: twitterUserId,
            name: twitterUserKey,
            username: twitterUsername,
          },
        ],
      });
    },
  );
}
