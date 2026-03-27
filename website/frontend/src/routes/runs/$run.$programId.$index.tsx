import { createFileRoute } from '@tanstack/react-router'
import { Button } from '../../components/Button.js'
import ArrowLeft from '../../assets/icons/arrow-left.svg?react'
import Fullscreen from '../../assets/icons/fullscreen.svg?react'
import { styled } from '@linaria/react'
import { text } from '../../fonts.js'
import { StatusChip } from '../../components/StatusChip.js'
import { MiniCard } from '../../components/MiniCard.js'
import { RadialGraph } from '../../components/RadialGraph.js'
import { c, formatNumber, metricToGraph } from '../../utils.js'
import { ResponsiveLineGraph } from '../../components/Chart.js'
import { useMemo, useState } from 'react'
import { css } from '@linaria/core'
import { InfoChit } from '../../components/InfoChit.js'
import {
	runHasState,
	RunStateIndicator,
} from '../../components/RunStateIndicator.js'
import { CheckpointButton } from '../../components/CheckpointButton.js'
import { fetchRunStreaming } from '../../fetchRuns.js'
import { PromptResults } from '../../components/PromptResults.js'
import { useStreamingLoaderData } from '../../useStreamingData.js'
import { RunBox } from '../../components/RunBox.js'
import { FullPagePortal } from '../../components/FullPagePortal.js'
import { ApiGetRun } from 'shared'
import AnimatedTokensCounter from '../../components/AnimatedTokensCounter.js'
export const Route = createFileRoute('/runs/$run/$programId/$index')({
	loader: async ({ params }) =>
		fetchRunStreaming(params.run, params.index, params.programId),
	component: RouteComponent,
})

function RouteComponent() {
	const runData = useStreamingLoaderData<ApiGetRun>({
		from: '/runs/$run/$programId/$index',
	})
	const run = runData?.run
	const isOnlyRun = runData?.isOnlyRun

	const backButton = (
		<Button
			style="action"
			icon={{
				side: 'left',
				svg: ArrowLeft,
			}}
			to={'/runs'}
		>
			back
		</Button>
	)
	const graphData = useMemo(() => {
		if (run) {
			const {
				promptResults,
				promptIndex,
				cumulativePromptResults,
				...historyForGraph
			} = run.metrics.history
			const graphs = metricToGraph(historyForGraph)
			if (graphs.evals) {
				for (const vals of Object.values(graphs.evals)) {
					if (Array.isArray(vals)) {
						for (const val of vals) {
							val.y *= 100
						}
					}
				}
			}
			return graphs
		}
	}, [run])

	const info = run?.info

	const goodEvals = useMemo(() => {
		if (!run) {
			return {}
		}
		return Object.fromEntries(
			Object.entries(run.metrics.summary.evals).filter(
				(arr): arr is [string, number] => arr[1] !== null
			)
		)
	}, [run?.metrics.summary.evals])

	const [fullscreen, setFullscreen] = useState(false)

	if (import.meta.env.VITE_DISABLE) {
		return (
			<RunContainer>
				{backButton}
				<RunBox
					title={
						<span className={text['display/4xl']}>temporarily unavailable</span>
					}
				>
					<div
						className={c(
							css`
								padding: 48px;
								text-align: center;
							`,
							text['body/base/regular']
						)}
					>
						Sorry, the Psyche website is experiencing issues right now. Please
						check back later!
					</div>
				</RunBox>
			</RunContainer>
		)
	}

	if (!info) {
		return (
			<RunContainer>
				{backButton}
				<RunBox
					title={<span className={text['display/4xl']}>run not found</span>}
				>
					<div
						className={c(
							css`
								padding: 48px;
								text-align: center;
							`,
							text['body/base/regular']
						)}
					>
						Sorry! Try another run ID.
					</div>
				</RunBox>
			</RunContainer>
		)
	}
	return (
		<FullPagePortal open={fullscreen}>
			<RunContainer>
				{!isOnlyRun && (
					<Button
						style="action"
						icon={{
							side: 'left',
							svg: ArrowLeft,
						}}
						to={'/runs'}
					>
						back
					</Button>
				)}
				<RunBox
					title={
						<>
							<span className={text['display/4xl']}>
								{info.name || info.id}{' '}
								{info.isOnlyRunAtThisIndex ? '' : `(v${info.index + 1})`}
								{run.programId && ` - ${run.programId.slice(0, 12)}`}
							</span>
							<TitleRightInfo>
								<StatusChip status={info.status.type} style="minimal" />
								<Button
									className="fullscreenButton"
									onClick={() => setFullscreen(!fullscreen)}
									style="secondary"
									icon={{
										side: 'left',
										svg: Fullscreen,
									}}
								/>
							</TitleRightInfo>
						</>
					}
				>
					<RunContents className={text['body/base/medium']}>
						<MainContentContainer>
							<ContentColumn>
								<RunDescription>{info.description}</RunDescription>
								<InfoChits>
									<InfoChit label="params">
										{formatNumber(Number(info.size), 2)}
									</InfoChit>
									<InfoChit label="arch">{info.arch}</InfoChit>
									<InfoChit label="type">{info.type}</InfoChit>
								</InfoChits>

								{info.trainingStep && (
									<div
										className={c(
											css`
												text-align: center;
											`,
											text['display/3xl']
										)}
									>
										<AnimatedTokensCounter
											lastValue={info.trainingStep.tokensCompletedAtStartOfStep}
											lastTimestamp={info.trainingStep.startedAt.time}
											perSecondRate={info.trainingStep.lastTokensPerSecond}
											pausedAt={info.trainingStep.endedAt?.time}
										/>
										<div>tokens trained</div>
									</div>
								)}
								{run.state?.checkpoint && (
									<CheckpointButton checkpoint={run.state.checkpoint} />
								)}

								<PromptResults
									tokens={run.cumulativePromptResults || []}
									promptIndex={run.promptIndex || undefined}
								/>

								<StatsContainer>
									{Object.entries(goodEvals).length >= 3 && (
										<RadialContainer>
											<RadialGraph
												data={goodEvals}
												formatValue={(v) => `${+(v * 100).toFixed(2)}%`}
											/>
										</RadialContainer>
									)}
									<StatBoxes>
										{run.metrics.summary.loss !== null && (
											<MiniCard
												text="loss"
												value={`${run.metrics.summary.loss.toFixed(2)}`}
											/>
										)}
										{run.metrics.summary.tokensPerSecond !== null && (
											<MiniCard
												text="training rate"
												value={`${formatNumber(
													run.metrics.summary.tokensPerSecond,
													1,
													true
												)}tok/s`}
											/>
										)}
									</StatBoxes>
								</StatsContainer>
							</ContentColumn>
							{runHasState(run) && run.info.status.type !== 'completed' && (
								<RunStateActiveContainer
									className="liveContainer"
									active={
										run.info.status.type === 'active' ||
										run.info.status.type === 'waitingForMembers'
									}
								>
									<RunStateIndicator
										paused={run.info.status.type === 'paused'}
										state={run}
										recentTxs={run.recentTxs}
										disconnected={!!runData?.disconnected}
									/>
								</RunStateActiveContainer>
							)}
						</MainContentContainer>
						<HistoryContainer>
							{graphData && (
								<>
									{/* TODO: render confidence and perplexity */}
									<LineGraphContainer>
										<ResponsiveLineGraph
											renderValue={(x) => `${+x.toFixed(2)}`}
											xLabel="step"
											title="loss"
											line={{
												label: 'loss',
												points: graphData.loss,
											}}
										/>
									</LineGraphContainer>

									<LineGraphContainer>
										<ResponsiveLineGraph
											renderValue={(x) => formatNumber(x, 2)}
											xLabel="step"
											title="training speed"
											line={{
												label: 'training speed',
												points: graphData.tokensPerSecond,
												unit: ' tok/s',
											}}
										/>
									</LineGraphContainer>

									{graphData &&
										Object.entries(graphData.evals).map(([label, points]) => (
											<LineGraphContainer key={label}>
												<ResponsiveLineGraph
													renderValue={(x) => (+`${x.toFixed(2)}`).toString()}
													xLabel="step"
													title={`Model Evaluation: ${label}`}
													line={{
														label,
														points,
														unit: '%',
													}}
												/>
											</LineGraphContainer>
										))}
								</>
							)}
						</HistoryContainer>
					</RunContents>
				</RunBox>
			</RunContainer>
		</FullPagePortal>
	)
}

const RunContainer = styled.div`
	padding: 0 24px;
	container-type: inline-size;
	height: 100%;

	@container (width < 400px) {
		padding: 0 8px;
	}
	@container (width < 350px) {
		padding: 0 2px;
	}
`

const TitleRightInfo = styled.div`
	display: flex;
	gap: 24px;
	button {
		margin: 4px 0;
	}
	@media (width <= 768px) {
		.fullscreenButton {
			display: none;
		}
	}
`

const StatBoxes = styled.div`
	display: flex;
	gap: 40px;
	padding: 32px;
	align-items: center;
	justify-content: center;
	flex-wrap: wrap;
`

const RadialContainer = styled.div`
	aspect-ratio: 1 / 1;
	height: 100%;
	width: 100%;
	max-width: min(384px, calc(100cqw - 64px));
`

const MainContentContainer = styled.div`
	display: flex;
	flex-direction: column;
	gap: 24px;
	width: 100%;

	@container (min-width: 1400px) {
		flex-direction: row;
		gap: 48px;
		align-items: flex-start;
	}
`

const ContentColumn = styled.div`
	display: flex;
	flex-direction: column;
	gap: 24px;
	flex: 1;
	min-width: 0;
	align-items: center;
	& > * {
		margin: 0 24px;
	}

	@container (min-width: 1800px) {
		flex: 0 0 calc(50% - 24px);
		max-width: calc(50% - 24px);
	}
`

const StatsContainer = styled.div`
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	width: 100%;

	@container (min-width: 900px) {
		flex-direction: row;
		justify-content: center;
		align-items: center;
	}
`

const RunContents = styled.div`
	flex-basis: 100%;
	flex-shrink: 0;
	flex-grow: 1;
	display: flex;
	flex-direction: column;
	gap: 24px;
	padding: 24px 0;
	overflow: hidden;
	align-items: center;
	& > *:not(${MainContentContainer}) {
		margin: 0 24px;
	}

	@container (width < 400px) {
		padding: 24px 8px;
	}
`

const HistoryContainer = styled.div`
	display: flex;
	flex-wrap: wrap;
	gap: 24px;
	& > * {
		flex: 1 0 128px;
	}
`
const LineGraphContainer = styled.div`
	height: 128px;
	min-width: 256px;
	margin: 16px;
`

const RunDescription = styled.span`
	word-break: break-word;
`

const InfoChits = styled.div`
	display: flex;
	gap: 24px;
`

const RunStateActiveContainer = styled.div`
	opacity: ${(props) => (props.active ? 1 : 0.5)};
	flex: 1;
	min-width: 0;

	@container (min-width: 1800px) {
		flex: 0 0 calc(50% - 24px);
		max-width: calc(50% - 24px);
	}
`
