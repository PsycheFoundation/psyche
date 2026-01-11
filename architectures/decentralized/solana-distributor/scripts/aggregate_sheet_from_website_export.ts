import { PrivyClient } from "@privy-io/node";
import { Pubkey, pubkeyFromBase58 } from "solana-kiss";
import { parseHtmlTable } from "./utils/parseHtmlTable";
import { resolveUserWalletForEmailAddress } from "./utils/resolveUserWalletForEmailAddress";
import { resolveUserWalletForGithubUsername } from "./utils/resolveUserWalletForGithubUsername";
import { resolveUserWalletForTwitterUsername } from "./utils/resolveUserWalletForTwitterUsername";

const whitelistFolder = process.argv[2];

const privyClient = new PrivyClient({
  appId: process.argv[3],
  appSecret: process.argv[4],
});

const twitterBearerToken = process.argv[5];

const tabVipGithubName = "Github Whitelist";
const tabPaperGithubName = "Paper Whitelist (Github)";
const tabAttroposGithubName = "Atropos Contributors Whitelist";

const tabVipTwitterName = "X Whitelist";
const tabPaperTwitterName = "Paper Whitelist (Twitter)";

const tabPaperLinkedInName = "Paper Whitelist (LinkedIn)";
const tabNousApiEmailName = "Nous API Whitelist";
const tabMiningPoolName = "Mining Pool Contributors Whitelist";
const tabNousDiscordName = "Discord Whitelist";

main();

async function main() {
  //console.log(tabVipGithubName, parseHtmlTable(tabVipGithubName));
  //console.log(tabPaperGithubName, parseHtmlTable(tabPaperGithubName));
  //console.log(tabPaperLinkedInName, parseHtmlTable(tabPaperLinkedInName));
  //console.log(tabPaperTwitterName, parseHtmlTable(tabPaperTwitterName));
  //console.log(tabVipTwitterName, parseHtmlTable(tabVipTwitterName));
  //console.log(tabNousApiName, parseHtmlTable(tabNousApiName));
  //console.log(tabMiningPoolName, parseHtmlTable(tabMiningPoolName));
  //console.log(tabAttroposName, parseHtmlTable(tabAttroposName));
  //console.log(tabDiscordName, parseHtmlTable(tabDiscordName));

  for (const tabMiningPoolRow of parseHtmlTable(
    whitelistFolder,
    tabMiningPoolName,
  )) {
    const solanaAddress = toPubkey(tabMiningPoolRow[0]);
    if (solanaAddress === null) {
      continue;
    }
    console.log(">>>> solanaAddress", solanaAddress);
    break;
  }

  for (const tabNousApiRow of parseHtmlTable(
    whitelistFolder,
    tabNousApiEmailName,
  )) {
    if (tabNousApiRow.length < 2) {
      continue;
    }
    const emailAddress = tabNousApiRow[0];
    if (emailAddress.indexOf("@") === -1) {
      continue;
    }
    console.log("emailAddress", emailAddress);
    const res = await resolveUserWalletForEmailAddress(
      privyClient,
      emailAddress,
    );
    console.log(">>>> EMAIL (Nous API)", JSON.stringify(res, null, 2));
    break;
  }

  for (const tabAttroposGithubRow of parseHtmlTable(
    whitelistFolder,
    tabAttroposGithubName,
  )) {
    const url = toUrl(tabAttroposGithubRow[0]);
    if (url === null) {
      continue;
    }
    const githubUsername = stripPrefix(url.pathname, "/");
    console.log("githubUsername", githubUsername);
    const res = await resolveUserWalletForGithubUsername(
      privyClient,
      githubUsername,
    );
    console.log(">>>>> GITHUB", JSON.stringify(res, null, 2));
    break;
  }

  /*
  for (const tabNousDiscordRow of parseHtmlTable(
    whitelistFolder,
    tabNousDiscordName
  )) {
    const discordUsername = tabNousDiscordRow[0]
    if (!discordUsername) {
      continue
    }
    console.log('discordUsername', discordUsername)
    const res = await resolveUserInfoForDiscordUsername(discordUsername)
    console.log('DISCORD', JSON.stringify(res, null, 2))
    break
  }

  for (const paperLinkedIn of parseHtmlTable(whitelistFolder,tabPaperLinkedInName)) {
    const url = toUrl(paperLinkedIn[0]);
    if (url === null) {
      continue;
    }
    const linkedInId = stripPrefix(url.pathname, "/in/");
    console.log("linkedInId", linkedInId);
    const res = await resolveInfoForLinkedInId(linkedInId);
  }
    */

  for (const paperTwitter of parseHtmlTable(
    whitelistFolder,
    tabPaperTwitterName,
  )) {
    const url = toUrl(paperTwitter[0]);
    if (url === null) {
      continue;
    }
    const twitterUsername = stripPrefix(url.pathname, "/");
    console.log("twitter", twitterUsername);
    const res = await resolveUserWalletForTwitterUsername(
      privyClient,
      twitterUsername,
      twitterBearerToken,
    );
    console.log(">>>>> TWITTER", JSON.stringify(res, null, 2));
    break;
  }
}

/*
async function resolveUserInfoForDiscordUsername(discordUsername: string) {
  return await resolveUserWalletOrPregenerate(
    discordUsername,
    (discordUsername) =>
      privyClient.users().getByDiscordUsername({ username: discordUsername }),
    async (fetchError) => {
      const discordUserInfo = await fetchJson(
        `https://discord.com/api/v10/users/${encodeURIComponent(
          discordUsername
        )}`,
        'GET'
      )
      console.log('discordUserInfo', discordUserInfo)
      const discordUserId = jsonAsNumber(jsonGetAt(discordUserInfo, 'id'))
      if (!discordUserId) {
        throw new ErrorStack(
          'Failed to fetch Discord user info: ' + discordUserInfo,
          fetchError
        )
      }
      return privyClient.users().create({
        linked_accounts: [
          {
            type: 'discord_oauth',
            subject: discordUserId.toString(),
            username: discordUsername,
          },
        ],
        wallets: [{ chain_type: 'solana' }],
      })
    }
  )
}

async function resolveInfoForLinkedInId(linkedInId: string) {
  return await resolveUserWalletOrPregenerate(
    privyClient,
    linkedInId,
    (linkedInId) => privyClient.users().getByLinkedInId({ id: linkedInId }),
    async (fetchError) => {
      const githubUserInfo = await fetchJson(
        `https://api.github.com/users/${encodeURIComponent(linkedInId)}`,
        'GET'
      )
      const githubUserId = jsonAsNumber(jsonGetAt(githubUserInfo, 'id'))
      if (!githubUserId) {
        throw new ErrorStack(
          'Failed to fetch GitHub user info: ' + githubUserInfo,
          fetchError
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
        wallets: [{ chain_type: 'solana' }],
      })
    }
  )
}

*/

function toUrl(urlString: string) {
  try {
    return new URL(urlString);
  } catch {
    return null;
  }
}

function toPubkey(value: string): Pubkey | null {
  try {
    return pubkeyFromBase58(value);
  } catch {
    return null;
  }
}

function stripPrefix(value: string, prefix: string): string {
  return value.startsWith(prefix) ? value.slice(prefix.length) : value;
}
