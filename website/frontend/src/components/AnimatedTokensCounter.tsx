import React, { useEffect, useMemo, useRef, useState } from 'react'
import { styled } from '@linaria/react'

const StyledDigit = styled.div`
	display: inline-block;
	font-variant-numeric: tabular-nums;
	filter: var(--blur-filter);
	width: 1ch;
	text-align: center;
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

class AnimatedCounter {
	private lastValue: number
	private lastTimestamp: Date
	private perSecondRate: number
	private pausedAt?: Date
	private currentAnimatedValue: number

	constructor(
		lastValue: bigint,
		lastTimestamp: Date,
		perSecondRate: bigint,
		pausedAt?: Date
	) {
		this.lastValue = Number(lastValue)
		this.lastTimestamp = lastTimestamp
		this.perSecondRate = Number(perSecondRate)
		this.pausedAt = pausedAt
		this.currentAnimatedValue = this.calculateTargetValue()
	}

	calculateTargetValue(): number {
		if (this.pausedAt) {
			const elapsedMs = Math.max(
				0,
				this.pausedAt.getTime() - this.lastTimestamp.getTime()
			)
			const elapsedSeconds = elapsedMs / 1000
			const increment = Math.floor(elapsedSeconds * this.perSecondRate)
			return this.lastValue + increment
		} else {
			const now = Date.now()
			const elapsedMs = now - this.lastTimestamp.getTime()
			const elapsedSeconds = elapsedMs / 1000
			const FUDGE_FACTOR = 0.9
			const increment = Math.floor(
				elapsedSeconds * this.perSecondRate * FUDGE_FACTOR
			)
			return this.lastValue + increment
		}
	}

	interpolate(deltaTime: number): number {
		const targetValue = this.calculateTargetValue()
		const easingFactor = Math.min(1, (deltaTime / 1000) * 3)
		this.currentAnimatedValue =
			this.currentAnimatedValue +
			(targetValue - this.currentAnimatedValue) * easingFactor
		return Math.round(this.currentAnimatedValue)
	}

	shouldContinueAnimating(): boolean {
		const targetValue = this.calculateTargetValue()
		return (
			!this.pausedAt || Math.abs(this.currentAnimatedValue - targetValue) > 0.5
		)
	}

	update(
		lastValue: bigint,
		lastTimestamp: Date,
		perSecondRate: bigint,
		pausedAt?: Date
	) {
		this.lastValue = Number(lastValue)
		this.lastTimestamp = lastTimestamp
		this.perSecondRate = Number(perSecondRate)
		this.pausedAt = pausedAt
	}
}

const calculateBlurMask = (
	formattedDigits: string[],
	perSecondRate: number,
	pausedAt?: Date
): Set<number> => {
	const blurredIndices = new Set<number>()

	// Don't blur anything when paused
	if (pausedAt) {
		return blurredIndices
	}

	const digitOnlyString = formattedDigits.filter((c) => /\d/.test(c)).join('')

	formattedDigits.forEach((char, index) => {
		if (!/\d/.test(char)) return

		const digitPosition =
			digitOnlyString.length -
			formattedDigits.slice(0, index + 1).filter((c) => /\d/.test(c)).length
		const placeValue = Math.pow(10, Math.max(0, digitPosition))
		const timeToChangeDigit =
			perSecondRate > 0 ? (placeValue / perSecondRate) * 1000 : Infinity
		const minMsVisibleBeforeBlur = 500

		if (perSecondRate > 0 && timeToChangeDigit < minMsVisibleBeforeBlur) {
			blurredIndices.add(index)
		}
	})

	return blurredIndices
}

const useAnimatedCounter = (props: Props) => {
	const { lastValue, lastTimestamp, perSecondRate, pausedAt } = props
	const [displayValue, setDisplayValue] = useState(() => Number(lastValue))

	const counterRef = useRef<AnimatedCounter>(
		new AnimatedCounter(lastValue, lastTimestamp, perSecondRate, pausedAt)
	)
	const animationRef = useRef<number>(null)
	const digitRefs = useRef<HTMLDivElement[]>([])

	// update counter when props change
	useEffect(() => {
		counterRef.current.update(lastValue, lastTimestamp, perSecondRate, pausedAt)
	}, [lastValue, lastTimestamp, perSecondRate, pausedAt])

	useEffect(() => {
		let lastFrameTime = Date.now()

		const animate = () => {
			const now = Date.now()
			const deltaTime = now - lastFrameTime
			lastFrameTime = now

			const newValue = counterRef.current.interpolate(deltaTime)

			// Only update DOM if value changed significantly
			if (Math.abs(newValue - displayValue) >= 1) {
				setDisplayValue(newValue)
			}

			if (counterRef.current.shouldContinueAnimating()) {
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
	}, [lastValue, lastTimestamp, perSecondRate, pausedAt, displayValue])

	return { displayValue, digitRefs }
}

const AnimatedTokensCounter: React.FC<Props> = (props) => {
	const { locale = 'en-US' } = props
	const { displayValue, digitRefs } = useAnimatedCounter(props)
	const instanceId = useRef(Math.random().toString(36).slice(2, 11))

	const formattedDigits = useMemo(
		() => BigInt(displayValue).toLocaleString(locale).split(''),
		[displayValue, locale]
	)

	const blurMask = useMemo(
		() =>
			calculateBlurMask(
				formattedDigits,
				Number(props.perSecondRate),
				props.pausedAt
			),
		[formattedDigits, props.perSecondRate, props.pausedAt]
	)

	return (
		<>
			<BlurFilters>
				<defs>
					{formattedDigits.map((_, index) => {
						const blurAmount = blurMask.has(index)
							? Array.from(blurMask).indexOf(index) * 0.74
							: 0
						return (
							<filter
								key={index}
								id={`blur-filter-${instanceId.current}-${index}`}
							>
								<feGaussianBlur stdDeviation={`0 ${blurAmount}`} />
							</filter>
						)
					})}
				</defs>
			</BlurFilters>
			<CounterContainer>
				{formattedDigits.map((char, index) => (
					<StyledDigit
						key={index}
						ref={(el: HTMLDivElement) => (digitRefs.current[index] = el)}
						style={
							{
								'--blur-filter': blurMask.has(index)
									? `url(#blur-filter-${instanceId.current}-${index})`
									: 'none',
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

export default AnimatedTokensCounter
