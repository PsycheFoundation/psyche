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
	const { commit, initTime, coordinators, miningPool } = Route.useLoaderData()

	return (
		<Container>
			<ShadowCard disabled>
				<div className={text['display/2xl']}>Indexer Status</div>

				<div>commit: {commit}</div>
				{[
					...Object.entries(coordinators).map(
						([programId, coordinator]) =>
							[`coordinator (${programId})`, coordinator.chain] as const
					),
					['mining pool', miningPool.chain] as const,
				].map(([name, c]) => {
					const numRecentSlots = 1000
					const ratioRecentSlots = Math.min(
						1,
						Math.max(
							0,
							(c.indexedSlot - (c.chainSlotHeight - numRecentSlots)) /
								numRecentSlots
						)
					)
					return (
						<div>
							<h4>{name}</h4>
							<div>program id:</div>
							<pre>{c.programId}</pre>
							<div>network genesis block:</div>
							<pre>{c.networkGenesis}</pre>
							<div
								className={css`
									display: flex;
									margin-bottom: 4px;
									justify-content: space-between;
								`}
							>
								<div>
									{(ratioRecentSlots * 100).toFixed(1)}% of last{' '}
									{numRecentSlots} slots indexed
								</div>
								<div>
									slot {c.indexedSlot} / {c.chainSlotHeight}
								</div>
								<div>{c.chainSlotHeight - c.indexedSlot} slots behind</div>
							</div>
							<ProgressBar
								ratio={ratioRecentSlots}
								chunkWidth={-2}
								chunkHeight={24}
							/>
						</div>
					)
				})}
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
					{Object.entries(coordinators).flatMap(([programId, coordinator]) =>
						coordinator.trackedRuns.map((run) => (
							<StatusChip
								key={`${programId}-${run.id}-${run.index}`}
								status={run.status.type}
								style={'bold'}
								inverted
							>
								{run.id} (v{run.index + 1})
							</StatusChip>
						))
					)}
				</div>
			</ShadowCard>
		</Container>
	)
}
