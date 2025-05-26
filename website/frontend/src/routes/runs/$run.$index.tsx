import { createFileRoute } from '@tanstack/react-router'
import { Button } from '../../components/Button.js'
import ArrowLeft from '../../assets/icons/arrow-left.svg?react'
import Fullscreen from '../../assets/icons/fullscreen.svg?react'
import HuggingfaceIcon from '../../assets/icons/huggingface.svg?react'
import { styled } from '@linaria/react'
import { forest } from '../../colors.js'
import { text } from '../../fonts.js'
import { StatusChip } from '../../components/StatusChip.js'
import { Runtime } from '../../components/Runtime.js'
import { MiniCard } from '../../components/MiniCard.js'
import { RadialGraph } from '../../components/RadialGraph.js'
import { c, formatBytes, formatNumber, metricToGraph } from '../../utils.js'
import { ResponsiveLineGraph } from '../../components/Chart.js'
import { useMemo, useState } from 'react'
import { css } from '@linaria/core'
import { InfoChit } from '../../components/InfoChit.js'
import {
	runHasState,
	RunStateIndicator,
} from '../../components/RunStateIndicator.js'
import { fetchRunStreaming } from '../../fetchRuns.js'
import { useStreamingLoaderData } from '../../useStreamingData.js'
import { RunBox } from '../../components/RunBox.js'
import { Progress } from '../../components/ProgressWrapper.js'
import { FullPagePortal } from '../../components/FullPagePortal.js'
import { ApiGetRun } from 'shared'
export const Route = createFileRoute('/runs/$run/$index')({
	loader: async ({ params }) => fetchRunStreaming(params.run, params.index),
	component: RouteComponent,
})

function RouteComponent() {
	const runData = useStreamingLoaderData<ApiGetRun>({
		from: '/runs/$run/$index',
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
			const graphs = metricToGraph(run.metrics.history)
			for (const vals of Object.values(graphs.evals)) {
				for (const val of vals) {
					val.y *= 100
				}
			}
			return graphs
		}
	}, [run])

	const info = run?.info

	const pauses = useMemo(
		() => info?.pauseHistory.map((p) => [p[0], p[1].time] as const),
		[info?.pauseHistory]
	)

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
						<RunDescription>{info.description}</RunDescription>
						<InfoChits>
							<InfoChit label="params">
								{formatNumber(Number(info.size), 2)}
							</InfoChit>
							<InfoChit label="arch">{info.arch}</InfoChit>
							<InfoChit label="type">{info.type}</InfoChit>
						</InfoChits>
						<RuntimeLabel>
							runtime
							<Runtime
								start={info.startTime.time}
								pauses={pauses}
								end={
									info.status.type === 'completed'
										? info.status.at.time
										: undefined
								}
							/>
						</RuntimeLabel>
						<Progress
							size="big"
							current={Number(info.completedTokens)}
							total={Number(info.totalTokens)}
							chunkHeight={24}
							chunkWidth={21}
							label="tokens"
						/>
						{run.state?.checkpoint && (
							<Button
								style="secondary"
								center
								icon={{
									side: 'left',
									svg: HuggingfaceIcon,
									autoColor: false,
								}}
								href={`https://huggingface.co/${run.state.checkpoint.repo_id}/${run.state.checkpoint.revision ? `tree/${run.state.checkpoint.revision}` : ''}`}
								target="_blank"
							>
								View latest checkpoint: {run.state.checkpoint.repo_id}
							</Button>
						)}
						<StatsAndLiveRunContainer>
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

							{Object.entries(goodEvals).length >= 3 && (
								<RadialContainer>
									<RadialGraph
										data={goodEvals}
										formatValue={(v) => `${+(v * 100).toFixed(2)}%`}
									/>
								</RadialContainer>
							)}
							<StatBoxes>
								{/* // TODO: calculate confidence and perplexity */}
								{run.metrics.summary.loss !== null && (
									<MiniCard
										text="loss"
										value={`${run.metrics.summary.loss.toFixed(2)}`}
									/>
								)}
								{run.metrics.summary.bandwidth !== null && (
									<MiniCard
										text="bandwidth"
										value={`${formatBytes(
											run.metrics.summary.bandwidth,
											2,
											'bits'
										)}ps`}
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
						</StatsAndLiveRunContainer>
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

									<LineGraphContainer>
										<ResponsiveLineGraph
											renderValue={(x) => `${formatBytes(x, 0, 'bits')}`}
											xLabel="step"
											title="inter-node bandwidth"
											line={{
												label: 'bandwidth',
												points: graphData.bandwidth,
												unit: '/s',
											}}
										/>
									</LineGraphContainer>

									{Object.entries(graphData.evals).map(([label, points]) => (
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
`

const RuntimeLabel = styled.span`
	.theme-dark & {
		color: ${forest[300]};
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
	max-height: 384px;
	height: 100cqh;
	max-width: calc(100cqw - 64px);
`

const StatsAndLiveRunContainer = styled.div`
	display: grid;
	grid-template-columns: 1fr 1fr;
	.liveContainer {
		grid-column: 1/3;
		place-self: center stretch;
		min-width: 0;
	}
	gap: 0 48px;

	place-items: center;
	@container (min-width: 1280px) {
		.liveContainer {
			grid-column: 1;
		}
		grid-template-columns: minmax(auto, 900px) 1fr 1fr;
	}
	@container (max-width: 900px) {
		.liveContainer {
			grid-column: 1;
		}
		grid-template-columns: 1fr;
	}
`

const RunContents = styled.div`
	flex-basis: 100%;
	flex-shrink: 0;
	flex-grow: 1;
	overflow-y: auto;
	display: flex;
	flex-direction: column;
	gap: 24px;
	padding: 24px 0;
	overflow: hidden;
	& > *:not(${StatsAndLiveRunContainer}) {
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
`
