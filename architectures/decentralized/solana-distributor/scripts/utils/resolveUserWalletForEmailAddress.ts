import { PrivyClient } from "@privy-io/node";
import { resolveUserWallet } from "./resolveUserWallet";

export async function resolveUserWalletForEmailAddress(
  privyClient: PrivyClient,
  sourceTab: string,
  emailAddress: string,
) {
  return await resolveUserWallet(
    privyClient,
    `Email user: ${emailAddress} (${sourceTab})`,
    () => privyClient.users().getByEmailAddress({ address: emailAddress }),
    async () => {
      return privyClient.users().create({
        linked_accounts: [{ type: "email", address: emailAddress }],
      });
    },
  );
}
