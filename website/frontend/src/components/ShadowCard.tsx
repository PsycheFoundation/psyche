import { styled } from '@linaria/react'
import { forest } from '../colors.js'
import { AnchorHTMLAttributes, forwardRef, PropsWithChildren } from 'react'
import { createLink, CreateLinkProps } from '@tanstack/react-router'

const RunShadow = styled.div`
	.theme-light & {
		background: ${forest[700]};
	}
	.theme-dark & {
		background: ${forest[500]};
	}
	width: 100%;
	height: 100%;
	position: absolute;
	top: 8px;
	left: 8px;
	transform: translateZ(-10px);
`

const RunDarkShadow = styled.div`
	background: var(--color-bg);
	width: calc(100% - 4px);
	height: calc(100% - 4px);
	position: absolute;
	top: 2px;
	left: 2px;
`

const RunShadowContainer = styled.a`
	flex-grow: 1;

	text-decoration: none;

	border: 2px solid;

	padding: 16px;

	position: relative;

	cursor: pointer;
	background: var(--color-bg);

	.theme-light & {
		border-color: ${forest[700]};
	}
	.theme-dark & {
		border-color: ${forest[500]};
	}

	&:hover ${RunDarkShadow} {
		display: none;
	}

	transform-style: preserve-3d;
`

const RunLinkShadowContainer = createLink(
	forwardRef<HTMLAnchorElement, AnchorHTMLAttributes<HTMLAnchorElement>>(
		(props, ref) => <RunShadowContainer ref={ref} {...props} />
	)
)

const RunContainer = styled.div`
	display: flex;
	flex-direction: column;
	position: relative;
	gap: 16px;

	.theme-light & {
		color: var(--color-fg);
	}
	.theme-dark & {
		color: ${forest[300]};
	}
`
export function ShadowCard({
	children,
	...props
}: PropsWithChildren<CreateLinkProps>) {
	return (
		<RunLinkShadowContainer {...props}>
			<RunContainer>{children}</RunContainer>
			<RunShadow>
				<RunDarkShadow />
			</RunShadow>
		</RunLinkShadowContainer>
	)
}
