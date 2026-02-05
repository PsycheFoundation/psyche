import { PrivyClient } from '@privy-io/node'
import { writeFileSync } from 'node:fs'
import { Pubkey, pubkeyFromBase58, pubkeyToBase58 } from 'solana-kiss'
import { parseHtmlTable } from './utils/parseHtmlTable'

const privyClient = new PrivyClient({
	appId: process.argv[2],
	appSecret: process.argv[3],
})

const whitelistFolder = process.argv[4]
const outputCsvFile = process.argv[5]

const tabVipGithubName = 'Github Whitelist'
const tabPaperGithubName = 'Paper Whitelist (Github)'
const tabAtroposGithubName = 'Atropos Contributors Whitelist'

const tabVipTwitterName = 'X Whitelist'
const tabPaperTwitterName = 'Paper Whitelist (Twitter)'

const tabPaperLinkedInName = 'Paper Whitelist (LinkedIn)'
const tabNousApiEmailName = 'Nous API Whitelist'
const tabMiningPoolName = 'Mining Pool Contributors Whitelist'
const tabNousDiscordName = 'Discord Whitelist'

type Allocation = {
	type: string
	identifier: string
	address: Pubkey | null
	uiAmount: number
}

main()

async function main() {
	const allocations = new Array<Allocation>()

	parseGithubPage(allocations, tabVipGithubName)
	parseGithubPage(allocations, tabPaperGithubName)
	parseGithubPage(allocations, tabAtroposGithubName)

	parseTwitterPage(allocations, tabVipTwitterName)
	parseTwitterPage(allocations, tabPaperTwitterName)

	parseLinkedInPage(allocations, tabPaperLinkedInName)
	parseEmailPage(allocations, tabNousApiEmailName)
	parseWalletPage(allocations, tabMiningPoolName)
	parseDiscordPage(allocations, tabNousDiscordName)

	let totalUiAmount = 0
	for (const allocation of allocations) {
		totalUiAmount += allocation.uiAmount
	}
	console.log('Total allocated tokens:', totalUiAmount)
	console.log('Total allocations:', allocations.length)

	allocations.sort((a, b) => {
		return b.uiAmount - a.uiAmount
	})

	const lines = []
	lines.push(
		[
			'Type',
			'User Identifier',
			'Allocated Tokens',
			'Wallet Id',
			'Wallet Address',
		].join(';')
	)

	for (let i = 0; i < allocations.length; i++) {
		const allocation = allocations[i]
		console.log(
			'-',
			`${(i + 1).toString().padStart(6, ' ')}`,
			'/',
			`${allocations.length.toString().padEnd(6, ' ')}`,
			'>',
			allocation.type.toString().padEnd(10, ' '),
			allocation.identifier.toString().padEnd(50, ' '),
			allocation.uiAmount
		)
		lines.push(
			[
				allocation.type,
				allocation.identifier,
				allocation.uiAmount,
				'',
				'',
			].join(';')
		)
		/*
    if (allocation.address !== null) {
      lines.push(
        [
          allocation.type,
          allocation.identifier,
          allocation.uiAmount,
          "",
          pubkeyToBase58(allocation.address),
        ].join(";"),
      );
    } else {
      const wallet = await privyClient
        .wallets()
        .create({ chain_type: "solana" });
      lines.push(
        [
          allocation.type,
          allocation.identifier,
          allocation.uiAmount,
          wallet.id,
          pubkeyToBase58(pubkeyFromBase58(wallet.address)),
        ].join(";"),
      );
    }
      */
	}

	writeFileSync(outputCsvFile, lines.join('\n'))
}

function parseEmailPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		const emailAddress = tabRow[0]
		const tokenUiAmount = Number(tabRow[1])
		allocations.push({
			type: 'email',
			identifier: emailAddress,
			address: null,
			uiAmount: tokenUiAmount,
		})
	}
}

function parseWalletPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		if (tabRow[0] === '') {
			continue
		}
		const solanaAddress = pubkeyFromBase58(tabRow[0])
		const tokenUiAmount = Number(tabRow[1])
		allocations.push({
			type: 'wallet',
			identifier: pubkeyToBase58(solanaAddress),
			address: solanaAddress,
			uiAmount: tokenUiAmount,
		})
	}
}

function parseGithubPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		const githubUsername = stripPrefix(new URL(tabRow[0]).pathname, '/')
		const tokenUiAmount = Number(tabRow[1])
		allocations.push({
			type: 'github',
			identifier: githubUsername,
			address: null,
			uiAmount: tokenUiAmount,
		})
	}
}

function parseTwitterPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		const url = new URL(tabRow[0])
		const tokenUiAmount = Number(tabRow[1])
		const twitterUsername = stripPrefix(url.pathname, '/')
		allocations.push({
			type: 'twitter',
			identifier: twitterUsername,
			address: null,
			uiAmount: tokenUiAmount,
		})
	}
}

function parseDiscordPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		if (tabRow[0] === '') {
			continue
		}
		const discordUserId = tabRow[0]
		const discordUserName = tabRow[1]
		const tokenUiAmount = Number(tabRow[2])
		allocations.push({
			type: 'discord',
			identifier: discordUserId + '#' + discordUserName,
			address: null,
			uiAmount: tokenUiAmount,
		})
	}
}

function parseLinkedInPage(allocations: Array<Allocation>, tabName: string) {
	for (const tabRow of parseHtmlTable(whitelistFolder, tabName)) {
		if (tabRow[0] === '#ERROR!') {
			continue
		}
		const url = new URL(tabRow[0])
		const linkedInId = stripPrefix(url.pathname, '/in/')
		const tokenUiAmount = Number(tabRow[1])
		allocations.push({
			type: 'linkedin',
			identifier: linkedInId,
			address: null,
			uiAmount: tokenUiAmount,
		})
	}
}

function stripPrefix(value: string, prefix: string): string {
	return value.startsWith(prefix) ? value.slice(prefix.length) : value
}
