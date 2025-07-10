import { useRef, useMemo, Fragment } from 'react'
import { Group } from '@visx/group'
import { Circle, LineRadial } from '@visx/shape'
import { scaleLinear } from '@visx/scale'
import { curveCatmullRomClosed } from '@visx/curve'
import { GridRadial, GridAngle } from '@visx/grid'
import { ParentSize } from '@visx/responsive'
import { useDarkMode } from 'usehooks-ts'
import { forest, slate } from '../colors.js'
import { Text, TextProps } from '@visx/text'
type FormatValue = (v: number) => string

export function RadialGraph({
	data,
	formatValue,
}: {
	data: Record<string, number>
	formatValue?: FormatValue
}) {
	return (
		<ParentSize debounceTime={10}>
			{({ width: visWidth, height: visHeight }) => (
				<RadialGraphInner
					data={data}
					width={visWidth}
					height={visHeight}
					formatValue={formatValue}
				/>
			)}
		</ParentSize>
	)
}

const graphPadding = 48
const textPadding = 8
const labelFontSize = 9
const labelFontOffset = labelFontSize * 0.6
const gridTicks = [0.25, 0.5, 0.75]

function RadialGraphInner({
	data,
	width,
	height,
	formatValue,
}: {
	data: Record<string, number>
	width: number
	height: number
	formatValue?: FormatValue
}) {
	const lineRef = useRef<SVGPathElement>(null)

	const minDimension = Math.min(width, height)
	const outerRadius = minDimension / 2 - graphPadding
	const innerRadius = Math.max(16, outerRadius * 0.3)

	const valueScale = scaleLinear<number>({
		domain: [0, 1],
		range: [innerRadius, outerRadius],
	})

	const { lineData, labelData } = useMemo(() => {
		const entries = Object.entries(data)
		const lineData = entries.map(([_k, v], i) => [i, v] as [number, number])
		const labelData = entries.map(([k, v], i) => ({ k, v, i }))
		return { lineData, labelData }
	}, [data])

	const indexAngleScale = useMemo(
		() =>
			scaleLinear({
				domain: [0, lineData.length],
				range: [0, Math.PI * 2],
			}),
		[lineData]
	)

	const labels = useMemo(
		() =>
			labelData.map(({ k, v, i }) => {
				const delta = indexAngleScale(i) - Math.PI / 2
				const rawAngle = delta * (180 / Math.PI)
				const inverted = rawAngle >= 90 && rawAngle < 270
				const flat =
					(rawAngle > -95 && rawAngle < -85) || (rawAngle > 85 && rawAngle < 95)
				const angle = flat ? 0 : rawAngle + (inverted ? 180 : 0)

				const textOffset = outerRadius + textPadding
				const basePosition = [
					Math.cos(delta) * textOffset,
					Math.sin(delta) * textOffset,
				]

				const position = flat
					? [
							[
								basePosition[0],
								basePosition[1] - (inverted ? -1 : 1) * labelFontSize,
							],
							basePosition,
						]
					: [
							[
								basePosition[0] + Math.cos(delta - 90) * labelFontOffset,
								basePosition[1] + Math.sin(delta - 90) * labelFontOffset,
							],
							[
								basePosition[0] + Math.cos(delta + 90) * labelFontOffset,
								basePosition[1] + Math.sin(delta + 90) * labelFontOffset,
							],
						]
				if (inverted) {
					position.reverse()
				}
				return {
					k,
					v,
					angle,
					position,
					inverted,
					flat,
				}
			}),
		[labelData, indexAngleScale, outerRadius]
	)

	const { isDarkMode } = useDarkMode()

	const curveColor = forest[isDarkMode ? 300 : 600]
	const labelColor = isDarkMode ? forest[300] : slate[1000]

	const radialLineColor = isDarkMode ? forest[300] : slate[500]
	const tickMiniColor = slate[isDarkMode ? 400 : 300]

	const gridCircleColor = isDarkMode ? forest[500] : slate[400]
	const innerCircleColor = isDarkMode ? forest[300] : slate[500]
	const outerCircleColor = isDarkMode ? forest[500] : slate[500]

	return (
		<>
			<svg
				width={width}
				height={height}
				style={{ overflow: 'visible', position: 'absolute' }}
			>
				<Group top={height / 2} left={width / 2}>
					<Group transform={`rotate(${180 / lineData.length})`}>
						<GridAngle
							scale={indexAngleScale}
							// from the outside,
							outerRadius={outerRadius}
							// to 1/2 the distance of the last inner tick
							innerRadius={
								innerRadius +
								(outerRadius - innerRadius) *
									(0.5 + 0.5 * gridTicks[gridTicks.length - 1])
							}
							stroke={tickMiniColor}
							strokeWidth={0.69}
							numTicks={lineData.length}
						/>
					</Group>
					<GridAngle
						scale={indexAngleScale}
						outerRadius={outerRadius}
						stroke={radialLineColor}
						strokeWidth={0.69}
						numTicks={lineData.length}
						innerRadius={innerRadius}
					/>
					<Group>
						<GridRadial
							scale={valueScale}
							tickValues={gridTicks}
							stroke={gridCircleColor}
							strokeWidth={1.58}
						/>
						<Circle
							r={innerRadius}
							fill="none"
							stroke={innerCircleColor}
							strokeWidth={1.58}
						/>
						<Circle
							r={outerRadius}
							fill="none"
							stroke={outerCircleColor}
							strokeWidth={1.58}
						/>
					</Group>
					<LineRadial
						angle={([index, _]: [number, number]) => indexAngleScale(index)}
						radius={(d: [number, number]) => valueScale(d[1])}
						curve={curveCatmullRomClosed}
					>
						{({ path }) => (
							<path
								d={path(lineData) ?? ''}
								ref={lineRef}
								strokeWidth={3}
								fill="none"
								stroke={curveColor}
							/>
						)}
					</LineRadial>
					{labels.map(({ k, v, angle, position, inverted, flat }) => {
						const textProps: TextProps = {
							angle,
							textAnchor: flat ? 'middle' : inverted ? 'end' : 'start',
							verticalAnchor: 'middle',
							fill: labelColor,
							fontSize: labelFontSize,
							fontFamily: 'Geist Mono',
						} as const

						return (
							<Fragment key={`${k}-label`}>
								<Text x={position[0][0]} y={position[0][1]} {...textProps}>
									{k}
								</Text>
								<Text x={position[1][0]} y={position[1][1]} {...textProps}>
									{formatValue ? formatValue(v) : +v.toFixed(2)}
								</Text>
							</Fragment>
						)
					})}
				</Group>
			</svg>
		</>
	)
}
