import { styled } from '@linaria/react'
import { RunData, RunRoundClient, RunState, TxSummary } from 'shared'
import { forest, gold, slate } from '../colors.js'
import { text } from '../fonts.js'
import { ProgressBar } from './ProgressBar.js'
import { css } from '@linaria/core'
import { useState } from 'react'
import { useInterval } from 'usehooks-ts'
import { c, solanaAccountUrl } from '../utils.js'
import { TxHistory } from './TxHistory.js'
import { Progress } from './ProgressWrapper.js'
import { Button } from './Button.js'
import { StatusChip } from './StatusChip.js'

const Container = styled.div`
	display: flex;
	gap: 0.5em;
	align-items: start;
	justify-content: stretch;
	& > * {
		flex-grow: 1;
		text-align: center;
	}
`

const stateNames: Record<RunState, string> = {
	Uninitialized: 'UNINITIALIZED',
	WaitingForMembers: 'WAITING FOR COMPUTE',
	Warmup: 'WARMUP',
	RoundTrain: 'TRAIN',
	RoundWitness: 'WITNESS',
	Cooldown: 'COOLDOWN',
	Finished: 'FINISHED',
	Paused: 'PAUSED',
}

const flexCol = css`
	flex-direction: column;
`

function calculateDoneRatio(
	state: Exclude<RunData['state'], undefined>,
	currentTime: Date
): number {
	const elapsedPhaseTime =
		(+currentTime -
			(state.phaseStartTime ? +state.phaseStartTime : Date.now())) /
		1000
	if (state.phase === 'WaitingForMembers') {
		return Math.min(state.clients.length / state.config.minClients, 1)
	} else if (state.phase === 'Warmup') {
		return Math.min(elapsedPhaseTime / state.config.warmupTime, 1)
	} else if (state.phase === 'Cooldown') {
		return Math.min(elapsedPhaseTime / state.config.cooldownTime, 1)
	} else if (state.phase === 'Finished') {
		return 1
	} else if (state.phase === 'Paused' || state.phase === 'Uninitialized') {
		return 0
	} else if (state.phase === 'RoundTrain') {
		const allWitnesses = state.clients.filter((c) => c.witness)
		const doneWitnesses = allWitnesses.filter((c) => c.witness === 'done')
		const witnessProgress = doneWitnesses.length / allWitnesses.length
		const timeProgress = elapsedPhaseTime / state.config.maxRoundTrainTime
		return Math.min(Math.max(witnessProgress, timeProgress), 1)
	} else if (state.phase === 'RoundWitness') {
		return Math.min(elapsedPhaseTime / state.config.roundWitnessTime, 1)
	} else {
		return 0
	}
}

const SectionsGrid = styled.div`
	display: grid;
	grid-template-columns: repeat(4, 1fr);
	@container (width < 480px) {
		grid-template-columns: repeat(2, 1fr);
	}
	gap: 8px;
`

const waitingForMembersBox = css`
	grid-column: 1 / -1;
`

const LegendBox = styled.div`
	span {
		span {
			margin-right: 4px;
		}
		display: inline-flex;
		align-items: center;
		padding-right: 1em;
	}
`

const DisconnectedBox = styled.div`
	position: absolute;
	left: 0;
	top: 0;
	right: 0;
	bottom: 0;
	height: 100%;
	background: rgb(from var(--color-bg) r g b / 95%);
	z-index: 2;
	display: flex;
	flex-direction: column;
	align-items: center;
	gap: 1em;
`

const Title = styled.div`
	display: flex;
	justify-content: space-between;
	align-items: center;
	padding: 4px 24px;
	.theme-light & {
		background: ${slate[300]};
	}
	.theme-dark & {
		background: ${forest[600]};
		color: ${forest[300]};
	}
`

const OuterContainer = styled.div`
	display: flex;
	flex-direction: column;
	gap: 24px;
	position: relative;
	& > *:not(.toEdge) {
		margin: 0 24px;
	}

	border-bottom: 2px solid;
	padding-bottom: 24px;
	.theme-dark & {
		border-color: ${forest[600]};
	}
	.theme-light & {
		border-color: ${slate[500]};
	}
`

export type RunWithState = Exclude<RunData, 'state'> & {
	state: Exclude<RunData['state'], undefined>
}

export function runHasState(run: RunData): run is RunWithState {
	return !!run.state
}

export function RunStateIndicator({
	state: { state },
	recentTxs,
	disconnected,
	paused,
}: {
	paused: boolean
	state: RunWithState
	recentTxs: TxSummary[]
	disconnected: boolean
}) {
	const {
		phase,
		config: { minClients, epochTime },
		clients,
		epochStartTime,
	} = state
	const [now, setNow] = useState(new Date(Date.now()))
	useInterval(() => setNow(new Date(Date.now())), 100)
	const doneRatio = calculateDoneRatio(state, now)
	return (
		<OuterContainer>
			<Title className="toEdge">
				<span className={text['aux/xl/medium']} title="the number of tokens">
					training status
					{clients
						? `: ${clients.length} node${clients.length === 1 ? '' : 's'} connected`
						: ''}
				</span>
				{!paused && (
					<span>
						<StatusChip status="active" style="minimal">
							live
						</StatusChip>
					</span>
				)}
			</Title>
			{disconnected && (
				<DisconnectedBox className="toEdge">
					<span>Disconnected from server, live run data is paused.</span>
					<Button style="secondary" onClick={() => window.location.reload()}>
						Reconnect
					</Button>
				</DisconnectedBox>
			)}
			<TxHistory
				txs={recentTxs.toReversed()}
				cluster={import.meta.env.VITE_COORDINATOR_CLUSTER}
			/>
			{phase !== 'Paused' && phase !== 'Uninitialized' && (
				<>
					{(phase === 'Warmup' ||
						phase === 'RoundTrain' ||
						phase === 'RoundWitness' ||
						phase === 'Cooldown') &&
						(() => {
							const elapsedMs = now.getTime() - epochStartTime.getTime()
							const elapsedSeconds = Math.max(0, Math.floor(elapsedMs / 1000))
							const isFinalizingEpoch =
								elapsedSeconds >= epochTime && elapsedSeconds > 0

							return (
								<Progress
									chunkHeight={8}
									chunkWidth={4}
									chunkSpacing={1}
									ratio={
										isFinalizingEpoch
											? 1
											: (now.getTime() - epochStartTime.getTime()) /
												(epochTime * 1000)
									}
									current={elapsedSeconds}
									total={epochTime}
									label={
										isFinalizingEpoch ? 'finalizing epoch' : 'epoch time (s)'
									}
									showRight={!isFinalizingEpoch}
								/>
							)
						})()}
					{phase === 'WaitingForMembers' && (
						<Progress
							chunkHeight={8}
							chunkWidth={4}
							chunkSpacing={1}
							current={clients.length}
							total={minClients}
							label="compute nodes"
						/>
					)}
					<LegendBox>
						<span>
							<Dot className="training" size="1em" />
							TRAINER
						</span>
						<span>
							<Dot className="waiting" size="1em" />
							UNFINISHED WITNESS
						</span>
						<span>
							<Dot className="done" size="1em" />
							FINISHED WITNESS
						</span>
					</LegendBox>
					<SectionsGrid>
						{phase === 'WaitingForMembers' ? (
							<>
								<Section
									active={phase === 'WaitingForMembers'}
									name={`${stateNames['WaitingForMembers']}`}
									className={waitingForMembersBox}
								/>
							</>
						) : (
							<>
								<Section
									active={phase === 'Warmup'}
									name={stateNames['Warmup']}
									doneRatio={doneRatio}
								/>
								<Section
									active={phase === 'RoundTrain'}
									name={stateNames['RoundTrain']}
									doneRatio={doneRatio}
								/>
								<Section
									active={phase === 'RoundWitness'}
									name={stateNames['RoundWitness']}
									doneRatio={doneRatio}
								/>

								<Section
									active={phase === 'Cooldown'}
									name={stateNames['Cooldown']}
									doneRatio={doneRatio}
								/>
							</>
						)}
					</SectionsGrid>
					<Container className={flexCol}>
						<ClientsBox>
							{clients.map((c, i) => (
								<RoundParticipant
									inRound={phase === 'RoundTrain' || phase === 'RoundWitness'}
									key={c.pubkey}
									client={c}
									index={i}
								/>
							))}
						</ClientsBox>
					</Container>
				</>
			)}
		</OuterContainer>
	)
}

const SectionBox = styled.div`
	display: flex;
	flex-direction: column;
	align-items: stretch;
	padding: 0.5em;
	border: 2px dotted;

	.theme-light & {
		border-color: ${slate[500]};
	}
	.theme-dark & {
		border-color: ${forest[500]};
	}

	justify-content: space-between;

	&.active {
		border-style: solid;
		background-color: ${forest[500]};
		color: ${slate[0]};
		border-color: rgba(255, 255, 255, 0.5) rgba(0, 0, 0, 0.5) rgba(0, 0, 0, 0.5)
			rgba(255, 255, 255, 0.5);
	}

	.title {
		text-align: left;
		padding-left: 0.25ch;
	}
`

function Section({
	active,
	name,
	doneRatio,
	className,
}: {
	active: boolean
	name: string
	doneRatio?: number
	className?: string
}) {
	return (
		<SectionBox className={c(active ? 'active' : '', className)}>
			<div className="title">{name}</div>
			{!!doneRatio && (
				<ProgressBar
					ratio={active ? doneRatio : 0}
					chunkWidth={2}
					chunkSpacing={1}
					chunkHeight={1}
					size="small"
					disabled={!active}
				/>
			)}
		</SectionBox>
	)
}

function RoundParticipant({
	client,
	index,
	inRound,
}: {
	inRound: boolean
	client: RunRoundClient
	index: number
}) {
	const expectedPubkeyLength = 44
	const segmentLength = Math.floor(expectedPubkeyLength / 4)
	const horSegLength = segmentLength + 5
	const vertSegLength = segmentLength - 5
	return (
		<a
			href={solanaAccountUrl(
				client.pubkey,
				import.meta.env.VITE_COORDINATOR_CLUSTER
			)}
			target="_blank"
			className="link"
		>
			<ClientBox className={text['body/xs/regular']} flicker={inRound}>
				<Dot
					className={!inRound ? 'training' : client.witness || 'training'}
					flickerDelay={(index * 12389123) % 32621}
				>
					<span className="center">8&times;H100</span>
				</Dot>
				<span className="invisible">
					{client.pubkey.slice(0, horSegLength)}
				</span>
				<span className="bottom">{client.pubkey.slice(0, horSegLength)}</span>
				<span className="right">
					{client.pubkey.slice(horSegLength, horSegLength + vertSegLength)}
				</span>
				<span className="top">
					{client.pubkey.slice(
						horSegLength + vertSegLength,
						horSegLength * 2 + vertSegLength
					)}
				</span>
				<span className="left">
					{client.pubkey.slice(horSegLength * 2 + vertSegLength)}
				</span>
			</ClientBox>
		</a>
	)
}

const Dot = styled.span`
	position: relative;
	height: ${(props) => props.size ?? '3em'};
	width: ${(props) => props.size ?? '6em'};
	border-radius: 0px;
	display: inline-block;
	&.training {
		background-color: ${slate[400]};
	}
	&.waiting {
		background-color: ${gold[400]};
	}
	&.done {
		background-color: ${forest[400]};
	}

	animation-delay: ${(props) => -(props.flickerDelay ?? 0)}ms;
`

const ClientBox = styled.div`
	max-width: 16ch;
	aspect-ratio: 1.5/1;

	position: relative;

	justify-content: space-between;

	.theme-light &,
	.theme-light & a {
		color: ${slate[500]};
	}
	.theme-dark &,
	.theme-dark & a {
		color: ${forest[500]};
	}

	.invisible {
		visibility: hidden;
		margin: 1ch;
	}
	.bottom {
		position: absolute;
		bottom: 0;
		right: 0;
		width: 100%;
	}

	.right {
		position: absolute;
		right: 0;
		bottom: 0;
		height: 100%;
		writing-mode: vertical-rl;
		text-orientation: mixed;
		transform: rotate(180deg);
	}

	.top {
		position: absolute;
		top: 0;
		right: 0;
		width: 100%;
		transform: rotate(180deg);
	}

	.left {
		position: absolute;
		left: 0;
		top: 0;
		height: 100%;
		writing-mode: vertical-lr;
		text-orientation: mixed;
	}

	.link {
		position: absolute;
		top: 50%;
		right: 50%;
		width: 1em;
		height: 1em;
	}

	${Dot} {
		.center {
			position: absolute;
			left: 0;
			top: 50%;
			transform: translateY(-50%);
			right: 0;
			color: ${forest[700]};
		}
		box-shadow:
			inset -1px -1px 0px rgba(0, 0, 0, 0.5),
			inset 1px 1px 0px rgba(255, 255, 255, 0.5);
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);

		animation-name: flicker;
		animation-duration: ${(props) => (props.flicker ? '2s' : 'none')};
		animation-timing-function: linear;
		animation-iteration-count: infinite;

		@keyframes flicker {
			0% {
				opacity: 1;
			}
			35% {
				opacity: 1;
			}
			50% {
				opacity: 0.45;
			}
			85% {
				opacity: 1;
			}
			100% {
				opacity: 1;
			}
		}
	}
`
const ClientsBox = styled.div`
	display: flex;
	flex-wrap: wrap;
	justify-content: space-between;
	width: 100%;
	gap: 1em;
	min-height: 82px;
	max-height: 50vh;
	overflow-y: scroll;
	padding: 0 8px;
	overflow-x: hidden;

	--bgRGB: 73, 89, 99;
	--bg: var(--color-bg);
	--bgTrans: rgb(from var(--color-bg) r g b / 0%);

	--shadow: rgba(41, 50, 56, 0.5);

	background:
    /* Shadow Cover TOP */
		linear-gradient(var(--bg) 30%, var(--bgTrans)) center top,
		/* Shadow Cover BOTTOM */ linear-gradient(var(--bgTrans), var(--bg) 70%)
			center bottom,
		/* Shadow TOP */
			radial-gradient(farthest-side at 50% 0, var(--shadow), rgba(0, 0, 0, 0))
			center top,
		/* Shadow BOTTOM */
			radial-gradient(
				farthest-side at 50% 100%,
				var(--shadow),
				rgba(0, 0, 0, 0)
			)
			center bottom;

	background-repeat: no-repeat;
	background-size:
		100% 40px,
		100% 40px,
		100% 14px,
		100% 14px;
	background-attachment: local, local, scroll, scroll;
`
