import { css } from '@linaria/core'
import { OverTime } from 'shared'

export const svgFillCurrentColor = css`
	display: inline-flex;
	& svg {
		width: 100%;
		height: 100%;
	}
	& path {
		fill: currentColor;
	}
`

export function c(...classes: Array<string | null | undefined | false>) {
	return classes.filter(Boolean).join(' ').trim()
}

export function formatNumber(
	num: number,
	decimals: number,
	space = false
): string {
	const suffixes = ['', 'k', 'm', 'b', 't', 'q']
	const suffixThresholds = [1, 1e3, 1e6, 1e9, 1e12, 1e15]

	if (num < 0) {
		return `-${formatNumber(-num, decimals, space)}`
	}

	if (num < 1000) {
		const fixed = num.toFixed(decimals)
		return fixed.replace(/\.?0+$/, '') + (space ? ' ' : '')
	}

	let suffixIndex = suffixes.length - 1
	while (suffixIndex > 0 && num < suffixThresholds[suffixIndex]) {
		suffixIndex--
	}

	const scaledNum = num / suffixThresholds[suffixIndex]
	const roundedNum = Math.floor(scaledNum * 10) / 10

	const fixed = roundedNum.toFixed(decimals)
	return (
		fixed.replace(/\.?0+$/, '') + (space ? ' ' : '') + suffixes[suffixIndex]
	)
}

export function formatBytes(
	bytes: number,
	fixed: number = 2,
	as: 'bytes' | 'bits' = 'bytes'
): string {
	const b = as === 'bits' ? 'b' : 'B'

	if (Number.isNaN(bytes)) {
		return `0 ${b}`
	}
	const KB = 1024.0
	const MB = KB * 1024.0
	const GB = MB * 1024.0
	const TB = GB * 1024.0
	const PB = TB * 1024.0

	if (bytes < KB) {
		return `${bytes.toFixed(0)} ${b}`
	}
	if (bytes < MB) {
		return `${(bytes / KB).toFixed(fixed)} ${as === 'bits' ? 'k' : 'K'}${b}`
	}
	if (bytes < GB) {
		return `${(bytes / MB).toFixed(fixed)} M${b}`
	}
	if (bytes < TB) {
		return `${(bytes / GB).toFixed(fixed)} G${b}`
	}
	if (bytes < PB) {
		return `${(bytes / TB).toFixed(fixed)} T${b}`
	}
	return `${(bytes / PB).toFixed(fixed)} P${b}`
}

export function formatUSDollars(money: number): string {
	return new Intl.NumberFormat('en-US', {
		style: 'currency',
		currency: 'USD',
	}).format(money)
}
export function mean(vals: number[]): number {
	return sum(vals) / vals.length
}
export function sum(vals: number[]): number {
	return vals.reduce((acc, v) => acc + v, 0)
}

type MappedObject<T extends object, V> = {
	[K in keyof T]: T[K] extends object ? MappedObject<T[K], V> : V
}
export function metricToGraph<
	T extends Record<string, number | Record<string, number>>,
>(data: OverTime<T>): MappedObject<T, Array<{ x: number; y: number }>> {
	const result: Record<string, any> = {}
	for (const [key, value] of Object.entries(data)) {
		if (Array.isArray(value)) {
			result[key] = value.map(([x, y]) => ({ x, y }))
		} else if (typeof value === 'object') {
			const nestedResults = metricToGraph(
				value as OverTime<Record<string, number | Record<string, number>>>
			)
			result[key] = nestedResults
		}
	}
	return result as MappedObject<T, Array<{ x: number; y: number }>>
}

export type SolanaCluster = 'mainnet' | 'devnet' | string

function solanaFmCluster(network: SolanaCluster) {
	if (network === 'mainnet') {
		return ''
	}
	if (network === 'devnet') {
		return 'devnet'
	}
	return `custom&customUrl=${encodeURIComponent(network)}`
}

export function solanaAccountUrl(account: string, network: SolanaCluster) {
	return `https://explorer.solana.com/address/${account}?cluster=${solanaFmCluster(network)}`
}

export function solanaTxUrl(tx: string, network: SolanaCluster) {
	return `https://explorer.solana.com/tx/${tx}?cluster=${solanaFmCluster(network)}`
}
