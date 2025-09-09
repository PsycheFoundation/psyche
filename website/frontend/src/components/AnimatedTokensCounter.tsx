import React, { useEffect, useLayoutEffect, useRef } from 'react'
import { styled } from '@linaria/react'

const StyledDigit = styled.div`
	display: inline-block;
	font-variant-numeric: tabular-nums;
	filter: var(--blur-filter);
`

const CounterContainer = styled.div`
	display: inline-flex;
	font-variant-numeric: tabular-nums;
	padding-top: 6px;
`

const BlurFilters = styled.svg`
	position: absolute;
	width: 0;
	height: 0;
	pointer-events: none;
`

interface Props {
	lastValue: bigint
	lastTimestamp: Date
	perSecondRate: bigint
	pausedAt?: Date
	className?: string
	locale?: string
}

const AnimatedCounter: React.FC<Props> = ({
	lastValue,
	lastTimestamp,
	perSecondRate,
	pausedAt,
	locale = 'en-US',
}) => {
	const containerRef = useRef<HTMLDivElement>(null)
	const digitRefs = useRef<(HTMLDivElement | null)[]>([])
	const animationRef = useRef<number | null>(null)
	const currentDigitsRef = useRef<string[]>([])
	const currentAnimatedValue = useRef<number>(Number(lastValue))
	const lastFrameTime = useRef<number>(Date.now())
	const currentAnimationRate = useRef<number>(0)

	const lerp = (start: number, end: number, factor: number) => {
		return start + (end - start) * factor
	}

	const updateDigitsInDOM = (value: bigint) => {
		const formattedValue = value.toLocaleString(locale)
		const newDigits = formattedValue.split('')

		const currentDigitElements = containerRef.current?.children || []
		if (currentDigitElements.length !== newDigits.length) {
			// rerender will handle this case
			return
		}

		digitRefs.current = Array.from(currentDigitElements) as HTMLDivElement[]

		newDigits.forEach((char, index) => {
			const element = digitRefs.current[index]
			if (element && currentDigitsRef.current[index] !== char) {
				element.textContent = char
			}
		})

		currentDigitsRef.current = newDigits

		// update blur filters in DOM ( no react rerender ! )
		const blurredDigitIndices = newDigits.reduce((acc, _, index) => {
			const digitOnlyString = Math.round(Number(value)).toString()
			const digitPosition =
				digitOnlyString.length -
				newDigits.slice(0, index + 1).filter((c) => /\d/.test(c)).length
			const placeValue = Math.pow(10, Math.max(0, digitPosition))
			const timeToChangeDigit =
				currentAnimationRate.current > 0
					? (placeValue / currentAnimationRate.current) * 1000
					: Infinity
			const minMsVisibleBeforeBlur = 500

			if (
				currentAnimationRate.current > 0 &&
				timeToChangeDigit < minMsVisibleBeforeBlur
			) {
				acc.push(index)
			}
			return acc
		}, [] as number[])

		const blurFilters = document.querySelector('svg defs')
		if (blurFilters) {
			blurFilters.innerHTML = newDigits
				.map((_, index) => {
					const blurredIndex = blurredDigitIndices.indexOf(index)
					const blurAmount = blurredIndex >= 0 ? blurredIndex * 0.54 : 0
					return `<filter id="blur-filter-${index}"><feGaussianBlur stdDeviation="0 ${blurAmount}" /></filter>`
				})
				.join('')
		}

		// update element styles to refer to correct blur filters
		for (let i = 0; i < newDigits.length; i++) {
			const element = digitRefs.current[i]
			if (element) {
				const shouldBlur = blurredDigitIndices.includes(i)
				element.style.setProperty(
					'--blur-filter',
					shouldBlur ? `url(#blur-filter-${i})` : 'none'
				)
			}
		}
	}

	const currentDigits = lastValue.toLocaleString(locale).split('')

	// update digitRefs when the component rerenders and digits refs change
	useLayoutEffect(() => {
		const currentDigitElements = containerRef.current?.childNodes || []
		digitRefs.current = Array.from(currentDigitElements) as HTMLDivElement[]

		currentDigitsRef.current = currentDigits
		// Initialize animated value to current value when component first loads
		currentAnimatedValue.current = Number(lastValue)
	}, [currentDigits.length])

	useEffect(() => {
		const animate = () => {
			const now = Date.now()
			const deltaTime = now - lastFrameTime.current
			lastFrameTime.current = now

			let targetValue: number

			if (pausedAt) {
				// If paused, calculate the final value at pause time
				const elapsedMs = Math.max(
					0,
					pausedAt.getTime() - lastTimestamp.getTime()
				)
				const elapsedSeconds = elapsedMs / 1000
				const increment = Math.floor(elapsedSeconds * Number(perSecondRate))
				targetValue = Number(lastValue) + increment
			} else {
				// If not paused, calculate the predicted current value
				const elapsedMs = now - lastTimestamp.getTime()
				const elapsedSeconds = elapsedMs / 1000
				const FUDGE_FACTOR = 0.9
				const increment = Math.floor(
					elapsedSeconds * Number(perSecondRate) * FUDGE_FACTOR
				)
				targetValue = Number(lastValue) + increment
			}

			// Smooth easing towards target value
			const easingFactor = Math.min(1, (deltaTime / 1000) * 3) // 3 units per second easing speed
			const previousValue = currentAnimatedValue.current
			currentAnimatedValue.current = lerp(
				currentAnimatedValue.current,
				targetValue,
				easingFactor
			)

			// Calculate actual animation rate (tokens per second)
			const valueChange = currentAnimatedValue.current - previousValue
			const timeInSeconds = deltaTime / 1000
			currentAnimationRate.current =
				timeInSeconds > 0 ? Math.abs(valueChange / timeInSeconds) : 0

			const displayValue = BigInt(Math.round(currentAnimatedValue.current))
			updateDigitsInDOM(displayValue)

			if (
				!pausedAt ||
				Math.abs(currentAnimatedValue.current - targetValue) > 0.5
			) {
				animationRef.current = requestAnimationFrame(animate)
			}
		}

		if (animationRef.current) {
			cancelAnimationFrame(animationRef.current)
		}

		animationRef.current = requestAnimationFrame(animate)

		return () => {
			if (animationRef.current) {
				cancelAnimationFrame(animationRef.current)
			}
		}
	}, [lastValue, lastTimestamp, perSecondRate, pausedAt, locale])

	return (
		<>
			<BlurFilters>
				<defs>
					{currentDigits.map((_, index) => (
						<filter key={index} id={`blur-filter-${index}`}>
							<feGaussianBlur stdDeviation="0 0" />
						</filter>
					))}
				</defs>
			</BlurFilters>
			<CounterContainer ref={containerRef}>
				{currentDigits.map((char, index) => (
					<StyledDigit
						key={index}
						ref={(el: HTMLDivElement) => (digitRefs.current[index] = el)}
						style={
							{
								'--blur-filter': 'none',
							} as React.CSSProperties
						}
					>
						{char}
					</StyledDigit>
				))}
			</CounterContainer>
		</>
	)
}

export default AnimatedCounter
