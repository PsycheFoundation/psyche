import { Axis } from '@visx/axis'
import { localPoint } from '@visx/event'
import { Group } from '@visx/group'
import { ParentSize } from '@visx/responsive'
import { scaleLinear, scalePower } from '@visx/scale'
import { Bar, Line, LinePath } from '@visx/shape'
import type React from 'react'
import { useCallback, useMemo, useState } from 'react'
import { GridOfPlusSymbols } from './ChartPlus.js'
import { useDarkMode } from 'usehooks-ts'
import { forest, lime, slate } from '../colors.js'
import { styled } from '@linaria/react'
import { text } from '../fonts.js'
import { css } from '@linaria/core'

interface DataPoint {
	x: number
	y: number
}

export interface GraphLine {
	points: DataPoint[]
	label: string
	unit?: string
}

interface LineGraphProps {
	xLabel: string
	line: GraphLine
	numXMarkers?: number
	numYMarkers?: number
	title?: string
	scale?: 'linear' | 'power'
	renderValue?: (x: number) => string
	forceMinX?: number
}

const margin = {
	right: 16,
	left: 24,
	bottom: 24,
}

const padding = {
	left: 8,
	bottom: 8,
}

const Tooltip = styled.div`
	position: absolute;
	z-index: 10;
	pointer-events: none;
	white-space: nowrap;
	padding: 2px 4px;
	text-align: center;
	background: ${forest[700]};
	color: ${slate[0]};
	overflow: visible;
`

const GraphContainer = styled.div`
	position: relative;
	height: 100%;
`

const Title = styled.div`
	color: var(--color-fg);
	pointer-events: none;
	display: flex;
	justify-content: space-between;
	padding: 0 4px;
	border-left: 1.5px solid ${(props) => props.color};
	border-right: 1.5px solid ${(props) => props.color};
	background:
		linear-gradient(
				to right,
				${(props) => props.color} 5px,
				transparent 5px,
				transparent calc(100% - 5px),
				${(props) => props.color} calc(100% - 5px)
			)
			bottom/100% 1.5px no-repeat,
		linear-gradient(
				to right,
				${(props) => props.color} 5px,
				transparent 5px,
				transparent calc(100% - 5px),
				${(props) => props.color} calc(100% - 5px)
			)
			top/100% 1.5px no-repeat;
`

const uppercase = css`
	text-transform: uppercase;
`
const WaitingForData = styled.div`
	display: flex;
	align-items: center;
	justify-content: center;
	height: 100%;
`

function findNiceDivisor(max: number, targetDivisions: number): number {
	// Calculate the rough step size
	const roughStep = max / targetDivisions

	// Find the magnitude of the rough step
	const magnitude = Math.pow(10, Math.floor(Math.log10(roughStep)))

	// Consider candidates: 1, 2, 5, 10 times the magnitude
	const candidates = [
		1 * magnitude,
		2 * magnitude,
		5 * magnitude,
		10 * magnitude,
	]

	// Find the candidate that gives the closest number of divisions to target
	let bestCandidate = candidates[0]
	let bestDiff = Math.abs(max / bestCandidate - targetDivisions)

	for (let i = 1; i < candidates.length; i++) {
		const divisions = max / candidates[i]
		const diff = Math.abs(divisions - targetDivisions)

		if (diff < bestDiff) {
			bestCandidate = candidates[i]
			bestDiff = diff
		}
	}

	return bestCandidate
}

function integerMarkers(
	domain: [number, number],
	targetMarkers: number,
	oneOver: boolean = false
): number[] {
	const [min, max] = domain
	const range = max - min

	const step = findNiceDivisor(range, targetMarkers)
	const firstMarker = Math.ceil(min / step) * step
	const markers: number[] = []

	for (let marker = firstMarker; marker <= max; marker += step) {
		markers.push(Number(marker.toFixed(10)))
	}
	if (oneOver && (markers.at(-1) ?? 0 < max)) {
		markers.push(firstMarker + markers.length * step)
	}

	return markers
}

// overcomplicated logic to make sure min & max are never both the same OR 0,
// lest we infinite loop trying to do tick markets.
function scaleDomainToEnsureSpace(nums: number[]): [number, number] {
	const minDomain = Math.min(...nums)
	const maxDomain = Math.max(...nums)
	return [
		minDomain,
		maxDomain === minDomain
			? minDomain === 0
				? 1
				: maxDomain * 1.1
			: maxDomain,
	]
}

const scaleFuncs: Record<'linear' | 'power', typeof scaleLinear> = {
	linear: scaleLinear,
	power: scalePower,
}

const LineGraphInner: React.FC<
	LineGraphProps & { width: number; height: number }
> = ({
	xLabel,
	renderValue,
	title: rawTitle,
	width,
	height,
	line,
	scale,
	forceMinX = 0,
}) => {
	const xMax = width - margin.left - margin.right - padding.left
	const yMax = height - margin.bottom - padding.bottom

	const scaleKind = scale === undefined ? 'linear' : scale

	const xScale = useMemo(
		() =>
			scaleLinear<number>({
				domain: scaleDomainToEnsureSpace([
					forceMinX,
					...line.points.map((d) => d.x),
				]),
				range: [0, xMax],
			}),
		[line, xMax]
	)

	const yScale = useMemo(
		() =>
			scaleFuncs[scaleKind]<number>({
				domain: scaleDomainToEnsureSpace(line.points.map((d) => d.y)),
				range: [yMax, 0],
				...(scaleKind === 'power'
					? {
							exponent: 10,
						}
					: {}),
			}),
		[scaleKind, yMax, line]
	)

	const [tooltipData, setTooltipData] = useState<{
		x: number
		y: number
		posX: number
		top: number
	} | null>(null)

	const handleTooltip = useCallback(
		(
			event: React.TouchEvent<SVGRectElement> | React.MouseEvent<SVGRectElement>
		) => {
			const { x } = localPoint(event) || { x: 0, y: 0 }
			const xValue = xScale.invert(x - margin.left - padding.left)

			// find the closest point on the line
			let closestPoint: DataPoint | null = null
			let minDistance = Number.POSITIVE_INFINITY

			const point = line.points.reduce((closest, current) => {
				const distance = Math.abs(current.x - xValue)
				return distance < Math.abs(closest.x - xValue) ? current : closest
			})

			const distance = Math.abs(point.x - xValue)
			if (distance < minDistance) {
				minDistance = distance
				closestPoint = point
			}

			if (closestPoint) {
				setTooltipData({
					x: closestPoint.x,
					y: closestPoint.y,
					posX: xScale(closestPoint.x) + margin.left + padding.left,
					top: yScale(closestPoint.y) + padding.bottom,
				})
			}
		},
		[xScale, line, yScale]
	)

	const handleMouseLeave = useCallback(() => {
		setTooltipData(null)
	}, [setTooltipData])

	const { isDarkMode } = useDarkMode()

	const title = rawTitle ?? `Line Graph of ${line.label} / ${xLabel}`

	const lineColor = forest[isDarkMode ? 300 : 600]
	const plusColor = slate[isDarkMode ? 500 : 300]
	const verticalGridColor = slate[isDarkMode ? 600 : 500]
	const labelColor = isDarkMode ? forest[300] : slate[1000]

	const xDomain = xScale.domain() as [number, number]
	const yDomain = yScale.domain() as [number, number]

	const numXMarkers = width / 96
	const numYMarkers = height / 32

	const verticalLinePositions = useMemo(
		() => integerMarkers([xDomain[0], xDomain[1]], numXMarkers),
		[xDomain, numXMarkers]
	)

	const horizontalLinePositions = useMemo(
		() => integerMarkers([yDomain[0], yDomain[1]], numYMarkers),
		[yDomain, numYMarkers]
	)

	if (line.points.length < 2) {
		return (
			<GraphContainer>
				{title && (
					<Title color={labelColor} className={text['body/sm/semibold']}>
						{title}
					</Title>
				)}
				<WaitingForData>waiting for data...</WaitingForData>
			</GraphContainer>
		)
	}

	const centerLineThickness = 1
	const plusSize = 6
	const plusThickness = 1

	const lastYValue = line.points.at(-1)!.y

	return (
		<GraphContainer>
			{title && (
				<Title color={labelColor} className={text['body/sm/semibold']}>
					<span className={uppercase}>{title}</span>
					<span>
						{(renderValue ?? ((x) => x.toFixed(1)))(lastYValue)}
						{line.unit}
					</span>
				</Title>
			)}
			<svg width={width} height={height} style={{ position: 'absolute' }}>
				<title>{title}</title>
				<Group left={margin.left + padding.left} top={padding.bottom}>
					{verticalLinePositions.map((x, i) => (
						<line
							key={i}
							stroke={verticalGridColor}
							x1={xScale(x)}
							y1={-plusSize}
							x2={xScale(x)}
							y2={yMax + plusSize}
							strokeWidth={centerLineThickness}
						/>
					))}

					<GridOfPlusSymbols
						stroke={plusColor}
						splitStroke={verticalGridColor}
						horizontalLinePositions={horizontalLinePositions}
						verticalLinePositions={verticalLinePositions}
						xScale={xScale}
						yScale={yScale}
						size={plusSize}
						thickness={plusThickness}
						centerLineThickness={0}
					/>
					<LinePath
						key={line.label}
						data={line.points}
						x={(d) => xScale(d.x)}
						y={(d) => yScale(d.y)}
						strokeWidth={1}
						stroke={lineColor}
						strokeLinecap="butt"
					/>
				</Group>

				<Group top={padding.bottom} left={margin.left + padding.left}>
					<Axis
						orientation="bottom"
						scale={xScale}
						top={yMax}
						tickValues={verticalLinePositions}
						hideTicks
						hideAxisLine
						tickComponent={({ formattedValue, ...tickProps }) => {
							return (
								formattedValue && (
									<text {...tickProps} dy={'0.5ch'} fill={labelColor}>
										{formattedValue}
									</text>
								)
							)
						}}
					/>
				</Group>
				<Group left={margin.left} top={padding.bottom}>
					<Axis
						orientation="left"
						scale={yScale}
						tickValues={horizontalLinePositions}
						hideAxisLine
						hideTicks
						tickFormat={(value) =>
							`${value.valueOf() >= 0 ? '' : '-'}${(
								renderValue ?? ((x) => x.toFixed(1))
							)(value.valueOf()).slice(0, 7)}`
						}
						tickComponent={({ formattedValue, ...tickProps }) => {
							return (
								formattedValue && (
									<text {...tickProps} dy={'0.5ch'} fill={labelColor}>
										{formattedValue}
									</text>
								)
							)
						}}
					/>
				</Group>
				<Bar
					x={margin.left + padding.left}
					y={padding.bottom}
					width={width}
					height={height}
					fill="transparent"
					onTouchStart={handleTooltip}
					onTouchMove={handleTooltip}
					onMouseMove={handleTooltip}
					onMouseLeave={handleMouseLeave}
				/>
				{tooltipData && (
					<g>
						<Line
							from={{ x: tooltipData.posX, y: padding.bottom }}
							to={{
								x: tooltipData.posX,
								y: height - margin.bottom,
							}}
							stroke={lineColor}
							strokeWidth={1}
							pointerEvents="none"
						/>
						<circle
							cx={tooltipData.posX}
							cy={tooltipData.top}
							r={4}
							fill={lime[500]}
							pointerEvents="none"
						/>
					</g>
				)}
			</svg>

			{tooltipData && (
				<Tooltip
					className={text['button/sm']}
					style={{
						top: '50%',
						transform: 'translate(0, -100%)',
						...(tooltipData.posX > width / 2
							? {
									right: width - (tooltipData.posX - 10),
								}
							: {
									left: tooltipData.posX + 10,
								}),
					}}
				>
					<div>{line.label}</div>
					<div>
						{xLabel} {tooltipData.x}:{' '}
						{(renderValue ?? ((x) => x.toFixed(2)))(tooltipData.y)}
						{line.unit ?? ''}
					</div>
				</Tooltip>
			)}
		</GraphContainer>
	)
}

export const ResponsiveLineGraph: React.FC<LineGraphProps> = (props) => {
	return (
		<ParentSize debounceTime={10}>
			{({ width, height }) => (
				<LineGraphInner width={width} height={height} {...props} />
			)}
		</ParentSize>
	)
}
