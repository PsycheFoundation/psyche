import { PrivyClient, User } from "@privy-io/node";
import { ErrorStack, Pubkey, pubkeyFromBase58 } from "solana-kiss";

export async function resolveUserWallet<P>(
  privyClient: PrivyClient,
  sourceDescription: string,
  userFetcher: (() => Promise<User>) | null,
  userFactory: () => Promise<User>,
) {
  try {
    if (userFetcher === null) {
      throw new ErrorStack("No user fetcher specified");
    }
    return getOrCreateUserWallet(
      privyClient,
      sourceDescription,
      await userFetcher(),
    );
  } catch (fetchError) {
    try {
      return getOrCreateUserWallet(
        privyClient,
        sourceDescription,
        await userFactory(),
      );
    } catch (createError) {
      throw new ErrorStack(
        "Failed to resolve or create user: " + String(sourceDescription),
        [fetchError, createError],
      );
    }
  }
}

async function getOrCreateUserWallet(
  privyClient: PrivyClient,
  sourceDescription: string,
  existingUser: User,
) {
  const existingWallet = getUserSolanaWalletAddress(existingUser);
  if (existingWallet) {
    return {
      sourceDescription: sourceDescription,
      privyUser: existingUser,
      walletAddress: existingWallet,
    };
  }
  const updatedUser = await privyClient
    .users()
    .pregenerateWallets(existingUser.id, {
      wallets: [{ chain_type: "solana" }],
    });
  const updatedWallet = getUserSolanaWalletAddress(updatedUser);
  if (updatedWallet) {
    return {
      sourceDescription: sourceDescription,
      privyUser: updatedUser,
      walletAddress: updatedWallet,
    };
  }
  throw new ErrorStack(
    "Failed to get or create a solana pregenerated wallet for user id: " +
      existingUser.id,
  );
}

function getUserSolanaWalletAddress(privyUser: User): Pubkey | null {
  for (const linkedAccount of privyUser.linked_accounts) {
    if (linkedAccount.type !== "wallet") {
      continue;
    }
    if (linkedAccount.chain_type !== "solana") {
      continue;
    }
    return pubkeyFromBase58(linkedAccount.address);
  }
  return null;
}
