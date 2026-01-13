import { PrivyClient } from "@privy-io/node";
import { resolveUserWallet } from "./resolveUserWallet";

export async function resolveUserWalletForDiscordUserInfo(
  privyClient: PrivyClient,
  sourceTab: string,
  discordUserInfo: {
    id: string;
    name: string;
  },
) {
  return await resolveUserWallet(
    privyClient,
    `Discord User: ${discordUserInfo.name} (${sourceTab})`,
    () =>
      privyClient
        .users()
        .getByDiscordUsername({ username: discordUserInfo.name }),
    async () => {
      return privyClient.users().create({
        linked_accounts: [
          {
            type: "discord_oauth",
            subject: discordUserInfo.id,
            username: discordUserInfo.name,
          },
        ],
      });
    },
  );
}
