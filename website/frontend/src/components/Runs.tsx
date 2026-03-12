import { styled } from '@linaria/react'
import { PropsWithChildren, useState, useMemo } from 'react'
import { RadioSelectBar } from './RadioSelectBar.js'
import { RunSummaryCard } from './RunSummary.js'
import { ApiGetRuns } from 'shared'
import { Sort } from './Sort.js'
import { text } from '../fonts.js'
import AnimatedTokensCounter from './AnimatedTokensCounter.js'
import { shouldDisplayRun } from '../allowlist.js'

const RunsContainer = styled.div`
	height: 100%;
	display: flex;
	flex-direction: column;
	position: relative;
	container-type: inline-size;
`

const RunsHeader = styled.div`
	padding: 0 24px;
	display: flex;
	flex-direction: row;
	flex-wrap: wrap;

	gap: 24px;
	padding-bottom: 1em;
	align-items: center;
	justify-content: space-between;
`

const RunBoxesContainer = styled.div`
	padding: 2px 24px;
	padding-bottom: 24px;
	display: flex;
	gap: 24px;
	display: grid;
	grid-template-columns: 1fr 1fr;
	@container (max-width: 866px) {
		grid-template-columns: 1fr;
	}
`

const GlobalStats = styled.div`
	padding: 1em 24px;
	display: flex;
	flex-wrap: wrap;
	gap: 1em;
`

const runTypes = [
	{ label: 'All', value: 'all' },
	{ label: 'Active', value: 'active' },
	{
		label: 'Completed',
		value: 'completed',
	},
	{ label: 'Paused', value: 'paused' },
] as const

const runSort = [
	{ label: 'Recently updated', value: 'updated' },
	{ label: 'size', value: 'size' },
] as const

type RunType = (typeof runTypes)[number]['value']

const sortFuncs = {
	updated: (a: ApiGetRuns['runs'][0], b: ApiGetRuns['runs'][0]) =>
		b.lastUpdate.time.getTime() - a.lastUpdate.time.getTime(),
	size: (a: ApiGetRuns['runs'][0], b: ApiGetRuns['runs'][0]) =>
		Number(b.size - a.size),
} as const

export function Runs({ runs }: ApiGetRuns) {
	const [runTypeFilter, setRunTypeFilter] = useState<RunType>('all')
	const [sort, setSort] = useState<(typeof runSort)[number]>(runSort[0])

	const filteredRuns = useMemo(
		() => runs.filter((r) => shouldDisplayRun(r.id)),
		[runs]
	)

	// Create stable sorted list that only changes when sort changes, not when data updates
	const sortedRuns = useMemo(() => {
		return [...filteredRuns].sort(sortFuncs[sort.value])
	}, [filteredRuns.length, sort.value]) // Only resort when sort changes or runs count changes

	const totalTokens = useMemo(
		() =>
			filteredRuns.reduce(
				(sum, run) =>
					sum + (run.trainingStep?.tokensCompletedAtStartOfStep ?? 0n),
				0n
			),
		[filteredRuns]
	)
	const totalTokensPerSecondActive = useMemo(
		() =>
			filteredRuns.reduce((sum, summary) => {
				const ACTIVE_TIMEOUT_MS = 10 * 60 * 1000
				const timeSinceLastUpdate =
					Date.now() - summary.lastUpdate.time.getTime()
				if (
					summary.status.type !== 'active' ||
					timeSinceLastUpdate > ACTIVE_TIMEOUT_MS
				) {
					return sum
				}
				return sum + (summary.trainingStep?.lastTokensPerSecond ?? 0n)
			}, 0n),
		[filteredRuns]
	)

	return (
		<RunsContainer>
			<GlobalStats>
				<GlobalStat label="tokens/sec">
					{totalTokensPerSecondActive.toLocaleString()}
				</GlobalStat>
				<GlobalStat label="tokens trained">
					<AnimatedTokensCounter
						lastValue={totalTokens}
						lastTimestamp={runs.reduce((d, r) => {
							if (r.lastUpdate.time > d) {
								return r.lastUpdate.time
							}
							return d
						}, new Date(0))}
						perSecondRate={totalTokensPerSecondActive}
						pausedAt={
							totalTokensPerSecondActive === 0n ? new Date(0) : undefined
						}
					/>
				</GlobalStat>
			</GlobalStats>
			<RunsHeader>
				<RadioSelectBar
					selected={runTypeFilter}
					options={runTypes}
					onChange={setRunTypeFilter}
				/>
				<Sort selected={sort} options={runSort} onChange={setSort} />
				{/* <Button style="secondary">train a new model</Button> */}
			</RunsHeader>
			<RunBoxesContainer>
				{sortedRuns
					.filter(
						(r) => runTypeFilter === 'all' || runTypeFilter === r.status.type
					)
					.map((r) => (
						<RunSummaryCard key={`id: ${r.id} index: ${r.index}`} info={r} />
					))}
			</RunBoxesContainer>
		</RunsContainer>
	)
}

const StatBox = styled.span`
	border: 2px solid currentColor;
	display: inline-flex;
	gap: 0.5em;
	align-items: center;
	padding: 0.5em;
`

function GlobalStat({ label, children }: PropsWithChildren<{ label: string }>) {
	return (
		<StatBox>
			<span className={text['display/2xl']}>{children}</span>
			<span className={text['body/sm/regular']}>{label}</span>
		</StatBox>
	)
}
