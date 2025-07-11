import { styled } from '@linaria/react'
import { text } from '../fonts.js'
import { forest } from '../colors.js'

const RunShadow = styled.div`
	background: var(--color-fg);
	.theme-dark & {
		background: ${forest[400]};
	}
	width: 100%;
	height: 100%;
	position: absolute;
	top: 8px;
	left: 8px;
	transform: translateZ(-10px);
`

const Shadow = styled.div`
	background: var(--color-bg);
	width: calc(100% - 4px);
	height: calc(100% - 4px);
	position: absolute;
	top: 2px;
	left: 2px;
`

const ShadowContainer = styled.a`
	flex-grow: 1;

	border: 2px solid var(--color-fg);

	.theme-dark & {
		border-color: ${forest[400]};
	}
	max-width: 216px;
	min-width: 164px;

	position: relative;

	background: var(--color-bg);

	transform-style: preserve-3d;

	display: flex;
	flex-direction: column;
	padding: 16px;
	gap: 16px;
`

export function MiniCard({
	value,
	text: body,
}: {
	value: string
	text: string
}) {
	return (
		<ShadowContainer>
			<span className={text['display/3xl']}>{value}</span>
			<span className={text['aux/sm/medium']}>{body}</span>
			<RunShadow>
				<Shadow />
			</RunShadow>
		</ShadowContainer>
	)
}
