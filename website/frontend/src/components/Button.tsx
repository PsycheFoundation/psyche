import {
	ButtonHTMLAttributes,
	FunctionComponent,
	AnchorHTMLAttributes,
} from 'react'
import { text } from '../fonts.js'
import { css, LinariaClassName } from '@linaria/core'
import { forest, gold, slate } from '../colors.js'
import { Link, LinkProps } from '@tanstack/react-router'
import { iconClass } from '../icon.js'
import { c } from '../utils.js'

const buttonStyle = css`
	text-decoration: none;

	padding: 0;
	display: inline-flex;
	align-items: center;
	justify-content: flex-start;

	border: none;
	cursor: pointer;
	text-transform: uppercase;

	&.center {
		justify-content: center;
	}
	& .contents {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	&:not(:has(.contents)) {
		padding: 3px 0;
	}

	box-shadow:
		inset -1px -1px 0px rgba(0, 0, 0, 0.5),
		inset 1px 1px 0px rgba(255, 255, 255, 0.5);

	&:disabled {
		cursor: not-allowed;
	}

	&:not(:disabled):active,
	&.fakePressed {
		box-shadow:
			inset -1px -1px 0px rgba(255, 255, 255, 0.5),
			inset 1px 1px 0px rgba(0, 0, 0, 0.5);
	}

	& div.icon {
		display: flex;
		align-self: stretch;
		align-items: center;
		justify-content: center;
	}

	& .icon svg {
		margin: 0px 3px;
		width: 0.9em;
		height: 0.9em;
	}

	& .contents {
		padding: 0 4px;
	}
	&:has(.left) .contents {
		padding-left: 0;
	}
	&:has(.right) .contents {
		padding-right: 0;
	}
`

const buttonPrimaryStyle = css`
	.theme-light & {
		background: ${gold[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[300]};
			color: ${slate[400]};
			text-shadow: 1px 1px 0px ${slate[200]};
		}
	}

	.theme-dark & {
		background: ${gold[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}
	}
`

const buttonSecondaryStyle = css`
	.theme-light & {
		background: ${slate[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[200]};
			color: ${slate[300]};
			text-shadow: 1px 1px 0px ${slate[100]};
		}
	}

	.theme-dark & {
		background: ${forest[500]};
		color: ${slate[0]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}
	}
`

const buttonThemeStyle = css`
	.theme-light & {
		background: ${slate[200]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[200]};
			color: ${slate[300]};
			text-shadow: 1px 1px 0px ${slate[100]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${slate[100]};
		}
	}

	.theme-dark & {
		background: ${forest[700]};
		color: ${slate[0]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${forest[600]};
		}
	}
`

const buttonActionStyle = css`
	border: 2px solid;
	background: transparent;
	border-color: var(--button-color);
	color: var(--button-color);

	.theme-light & {
		--button-color: ${forest[700]};
		--button-bg: ${slate[0]};

		&:not(:disabled):not(:active):not(.fakePressed):hover {
			--button-color: ${forest[400]};
		}

		&:disabled {
			--button-color: ${slate[300]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${forest[700]};
			color: ${slate[0]};
			border-color: ${forest[700]};
		}
	}

	.theme-dark & {
		--button-color: ${slate[0]};
		--button-bg: ${forest[700]};

		&:not(:disabled):not(:active):not(.fakePressed):hover {
			--button-color: ${slate[400]};
		}

		&:disabled {
			--button-color: ${forest[600]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${slate[0]};
			color: ${forest[700]};
			border-color: ${slate[0]};
		}
	}

	& div.icon {
		background: var(--button-color);
		color: var(--button-bg);
	}

	&,
	&:not(:disabled):active,
	&.fakePressed {
		box-shadow: none;
	}
	&:has(.left) .contents {
		padding-left: 4px;
	}
	&:has(.right) .contents {
		padding-right: 4px;
	}
`

interface ButtonProps {
	style: 'primary' | 'secondary' | 'theme' | 'action'
	icon?: {
		svg: FunctionComponent<React.SVGProps<SVGSVGElement>>
		side: 'left' | 'right'
		autoColor?: boolean
	}
	pressed?: boolean
	center?: boolean
}

const styles: Record<ButtonProps['style'], LinariaClassName> = {
	primary: buttonPrimaryStyle,
	secondary: buttonSecondaryStyle,
	theme: buttonThemeStyle,
	action: buttonActionStyle,
}

type HTMLButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'style'>
type HTMLAnchorProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'style'>

// TODO refactor to remove duplication
export function Button<T extends LinkProps | HTMLButtonProps | HTMLAnchorProps>(
	props: T & ButtonProps
) {
	const { style, pressed, center } = props
	const className = c(
		text['button/sm'],
		buttonStyle,
		styles[style],
		pressed && 'fakePressed',
		'className' in props && props.className,
		center && 'center'
	)

	if ('to' in props) {
		const {
			style: _,
			children,
			icon,
			pressed: __,
			center: ___,
			...buttonProps
		} = props as LinkProps & ButtonProps
		return (
			<Link {...buttonProps} className={className}>
				{({ isActive, isTransitioning }) => (
					<>
						{icon?.side === 'left' && (
							<div className="icon">
								<icon.svg
									className={`${icon.autoColor === false ? '' : iconClass} left`}
								/>
							</div>
						)}
						{children && (
							<span className="contents">
								{typeof children === 'function'
									? children({ isActive, isTransitioning })
									: children}
							</span>
						)}
						{icon?.side === 'right' && (
							<div className="icon">
								<icon.svg
									className={`${icon.autoColor === false ? '' : iconClass} right`}
								/>
							</div>
						)}
					</>
				)}
			</Link>
		)
	} else if ('href' in props) {
		const {
			style: _,
			children,
			icon,
			pressed: __,
			center: ___,
			...buttonProps
		} = props as HTMLAnchorProps & ButtonProps
		return (
			<a {...buttonProps} className={className}>
				{icon?.side === 'left' && (
					<div className="icon">
						<icon.svg
							className={`${icon.autoColor === false ? '' : iconClass} left`}
						/>
					</div>
				)}
				{children && <span className="contents">{children}</span>}
				{icon?.side === 'right' && (
					<div className="icon">
						<icon.svg
							className={`${icon.autoColor === false ? '' : iconClass} right`}
						/>
					</div>
				)}
			</a>
		)
	} else {
		const {
			style: _,
			children,
			icon,
			pressed: __,
			center: ___,
			...buttonProps
		} = props as HTMLButtonProps & ButtonProps
		return (
			<button {...buttonProps} className={className}>
				{icon?.side === 'left' && (
					<div className="icon">
						<icon.svg
							className={`${icon.autoColor === false ? '' : iconClass} left`}
						/>
					</div>
				)}
				{children && <span className="contents">{children}</span>}
				{icon?.side === 'right' && (
					<div className="icon">
						<icon.svg
							className={`${icon.autoColor === false ? '' : iconClass} right`}
						/>
					</div>
				)}
			</button>
		)
	}
}
