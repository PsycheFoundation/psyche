import { styled } from '@linaria/react'
import { text } from '../fonts.js'
import { formatNumber } from '../utils.js'
import { ProgressBar } from './ProgressBar.js'
import { forest } from '../colors.js'

const ProgressContainer = styled.div`
	display: flex;
	flex-direction: column;
`

const ProgressDescription = styled.div`
	display: flex;
	flex-direction: row;
	gap: 8px;
	justify-content: space-between;
	color: ${forest[700]};
	.theme-dark & {
		color: ${forest[200]};
	}
`
export function Progress({
	chunkWidth,
	chunkHeight,
	chunkSpacing,
	label,
	total,
	current,
	ratio,
	format = (num) => formatNumber(num, 3),
	size,
	showRight = true,
}: {
	chunkWidth: number
	chunkHeight: number
	chunkSpacing?: number
	label: string
	total: number
	current: number
	ratio?: number
	format?: (val: number) => string
	size?: 'normal' | 'big' | 'small'
	showRight?: boolean
}) {
	return (
		<ProgressContainer>
			<ProgressBar
				ratio={ratio ?? current / total}
				chunkHeight={chunkHeight}
				chunkWidth={chunkWidth}
				chunkSpacing={chunkSpacing}
				size={size}
			/>
			<ProgressDescription className={text['aux/sm/medium']}>
				<span>{label}</span>
				{showRight && (
					<span>
						{format(current)}/{format(total)}
					</span>
				)}
			</ProgressDescription>
		</ProgressContainer>
	)
}
