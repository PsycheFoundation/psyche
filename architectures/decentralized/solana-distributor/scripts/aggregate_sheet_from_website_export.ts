import { PrivyClient } from "@privy-io/node";
import { pubkeyFromBase58 } from "solana-kiss";
import { parseHtmlTable } from "./utils/parseHtmlTable";
import { resolveUserWalletForDiscordUserInfo } from "./utils/resolveUserWalletForDiscordUsername";
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
const tabAtroposGithubName = "Atropos Contributors Whitelist";

const tabVipTwitterName = "X Whitelist";
const tabPaperTwitterName = "Paper Whitelist (Twitter)";

const tabPaperLinkedInName = "Paper Whitelist (LinkedIn)";
const tabNousApiEmailName = "Nous API Whitelist";
const tabMiningPoolName = "Mining Pool Contributors Whitelist";
const tabNousDiscordName = "Discord Whitelist";

main();

async function processGithubPage(results: Array<any>, tabName: string) {
  for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
    const githubUsername = stripPrefix(new URL(tabRow[0]).pathname, "/");
    const tokenUiAmount = Number(tabRow[1]);
    console.log("Resolving github user:", githubUsername);
    const res = await resolveUserWalletForGithubUsername(
      privyClient,
      tabName,
      githubUsername,
    );
    results.push({
      userId: res.privyUser.id,
      signerAddress: res.walletAddress,
      uiAmount: tokenUiAmount,
      description: res.sourceDescription,
    });
    break;
  }
}

async function main() {
  const results: Array<any> = [];
  //console.log(tabVipGithubName, parseHtmlTable(tabVipGithubName));
  //console.log(tabPaperGithubName, parseHtmlTable(tabPaperGithubName));
  //console.log(tabPaperLinkedInName, parseHtmlTable(tabPaperLinkedInName));
  //console.log(tabPaperTwitterName, parseHtmlTable(tabPaperTwitterName));
  //console.log(tabVipTwitterName, parseHtmlTable(tabVipTwitterName));
  //console.log(tabNousApiName, parseHtmlTable(tabNousApiName));
  //console.log(tabMiningPoolName, parseHtmlTable(tabMiningPoolName));
  //console.log(tabAttroposName, parseHtmlTable(tabAttroposName));

  await processGithubPage(results, tabVipGithubName);
  await processGithubPage(results, tabPaperGithubName);
  await processGithubPage(results, tabAtroposGithubName);

  for (const tabMiningPoolRow of parseHtmlTable(
    whitelistFolder,
    tabMiningPoolName,
  )) {
    const solanaAddress = pubkeyFromBase58(tabMiningPoolRow[0]);
    const tokenUiAmount = Number(tabMiningPoolRow[1]);
    results.push({
      signerAddress: solanaAddress,
      uiAmount: tokenUiAmount,
      description: tabMiningPoolName,
    });
    break;
  }

  for (const tabNousApiRow of parseHtmlTable(
    whitelistFolder,
    tabNousApiEmailName,
  )) {
    const emailAddress = tabNousApiRow[0];
    const tokenUiAmount = Number(tabNousApiRow[1]);
    const res = await resolveUserWalletForEmailAddress(
      privyClient,
      tabNousApiEmailName,
      emailAddress,
    );
    results.push({
      userId: res.privyUser.id,
      signerAddress: res.walletAddress,
      uiAmount: tokenUiAmount,
      description: res.sourceDescription,
    });
    break;
  }

  for (const tabNousDiscordRow of parseHtmlTable(
    whitelistFolder,
    tabNousDiscordName,
  )) {
    const discordUserId = tabNousDiscordRow[0];
    const discordUserName = tabNousDiscordRow[1];
    const tokenUiAmount = Number(tabNousDiscordRow[2]);
    console.log("discordUserName", discordUserName);
    console.log("discordUserId", discordUserId);
    const res = await resolveUserWalletForDiscordUserInfo(
      privyClient,
      tabNousDiscordName,
      { id: discordUserId, name: discordUserName },
    );
    console.log(">>>> DISCORD", JSON.stringify(res, null, 2));
    results.push({
      userId: res.privyUser.id,
      signerAddress: res.walletAddress,
      uiAmount: tokenUiAmount,
      description: res.sourceDescription,
    });
    break;
  }

  /*
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

  for (const rowPaperTwitter of parseHtmlTable(
    whitelistFolder,
    tabPaperTwitterName,
  )) {
    const url = new URL(rowPaperTwitter[0]);
    const tokenUiAmount = Number(rowPaperTwitter[1]);
    const twitterUsername = stripPrefix(url.pathname, "/");
    const res = await resolveUserWalletForTwitterUsername(
      privyClient,
      tabPaperTwitterName,
      twitterUsername,
      twitterBearerToken,
    );
    console.log(">>>>> TWITTER", JSON.stringify(res, null, 2));
    results.push({
      signerAddress: res.walletAddress,
      uiAmount: tokenUiAmount,
      description: res.sourceDescription,
    });
    break;
  }

  console.log("results", results);
}

/*
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

function stripPrefix(value: string, prefix: string): string {
  return value.startsWith(prefix) ? value.slice(prefix.length) : value;
}
