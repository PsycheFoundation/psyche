import { createFileRoute } from '@tanstack/react-router'
import { fetchStatus } from '../fetchRuns.js'
import { ShadowCard } from '../components/ShadowCard.jsx'
import { styled } from '@linaria/react'
import { text } from '../fonts.js'
import { Runtime } from '../components/Runtime.jsx'
import { StatusChip } from '../components/StatusChip.jsx'
import { ProgressBar } from '../components/ProgressBar.jsx'
import { css } from '@linaria/core'

export const Route = createFileRoute('/status')({
	loader: fetchStatus,
	component: RouteComponent,
})

const Container = styled.div`
	display: flex;
	width: 100%;
	height: 100vh;
	align-items: center;
	justify-content: center;
`

function RouteComponent() {
	const { commit, initTime, coordinator, miningPool } = Route.useLoaderData()
	return (
		<Container>
			<ShadowCard disabled>
				<div className={text['display/2xl']}>Indexer Status</div>

				<div>commit: {commit}</div>
				{(
					[
						['coordinator', coordinator.chain],
						['mining pool', miningPool.chain],
					] as const
				).map(([name, c]) => (
					<div>
						<h4>{name}</h4>
						<div>program id:</div>
						<pre>{coordinator.chain.programId}</pre>
						<div>network genesis block:</div>
						<pre>{coordinator.chain.networkGenesis}</pre>
						<div
							className={css`
								display: flex;
								margin-bottom: 4px;
								justify-content: space-between;
							`}
						>
							<div>{(
								(c.indexedSlot / c.chainSlotHeight) *
								100
							).toFixed(4)}
							% indexed</div><div>slot {c.indexedSlot} /{' '}
							{c.chainSlotHeight}</div><div>
							{c.chainSlotHeight - c.indexedSlot} slots behind</div>
						</div>
						<ProgressBar
							ratio={c.indexedSlot / c.chainSlotHeight}
							chunkWidth={-2}
							chunkHeight={24}
						/>
					</div>
				))}
				<div className={text['aux/xs/regular']}>
					uptime
					<Runtime start={new Date(initTime)} />
				</div>
				<div
					className={css`
						display: flex;
						flex-wrap: wrap;
						align-items: center;
						gap: 8px;
					`}
				>
					tracked runs:
					{coordinator.trackedRuns.map((run) => (
						<StatusChip
							status={run.status.type}
							style={'bold'}
							inverted
						>
							{run.id}
						</StatusChip>
					))}
				</div>
			</ShadowCard>
		</Container>
	)
}
