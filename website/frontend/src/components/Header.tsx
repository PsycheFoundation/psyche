import { styled } from '@linaria/react'

import PsycheLogo from '../assets/icons/psyche-box.svg?react'
import Symbol02 from '../assets/icons/symbol-02.svg?react'
import CornerFleur from '../assets/icons/corner-fleur.svg?react'
import Sun from '../assets/icons/sun.svg?react'
import Moon from '../assets/icons/moon.svg?react'
import { css } from '@linaria/core'
import { text } from '../fonts.js'
import { Button } from './Button.js'
import { c, svgFillCurrentColor } from '../utils.js'
import { useDarkMode } from 'usehooks-ts'
import { iconClass } from '../icon.js'
import { SymbolSeparatedItems } from './SymbolSeparatedItems.js'
import { Link } from '@tanstack/react-router'

const smallBreakpoint = '872px'

const NavContainer = styled.div`
	display: flex;
	flex-direction: row;
	padding: 8px;
	background: var(--color-bg);
`

const NavMain = styled.div`
	display: grid;
	grid-template-columns: minmax(256px, 1fr) minmax(312px, 1fr) minmax(64px, 1fr);

	& > .blurb {
		justify-self: center;
		text-transform: uppercase;
	}

	& > .buttons {
		justify-self: end;
	}

	@media (width < ${smallBreakpoint}) {
		grid-template-columns: minmax(256px, 1fr) minmax(200px, 300px);
		padding-right: 128px;
		& > .buttons {
			justify-self: start;
			flex-direction: row;
		}

		& > .blurb {
			justify-self: start;
		}
	}
	@media (width < 722px) {
		grid-template-columns: 300px;
	}
	gap: 24px;
	width: 100%;
	padding: 24px;

	& a.homelink,
	a:visited.homelink {
		text-decoration: none;
		color: var(--color-fg);
	}
`

const VerticalStack = styled.div`
	display: flex;
	flex-direction: column;
	gap: 12px;
	width: fit-content;

	& span {
		display: inline-flex;
		align-items: center;
	}
`

const psycheLogo = css`
	width: 1em;
	height: 1em;
	padding-right: 12px;
`

const symbol02 = css`
	padding-left: 8px;
	height: 1.5em;
	width: 2em;
`
const Chip = styled.div`
	background: var(--color-fg);
	color: var(--color-bg);
	width: fit-content;
	padding: 0 4px;
`

const cornerFleur = css`
	min-width: 128px;
	height: 128px;

	@media (width < ${smallBreakpoint}) {
		position: absolute;
		overflow: visible;
		top: 8px;
		right: 0;
	}

	@media (width < 380px) {
		width: 25vw;
		height: 25vw;
	}
`
export function Header() {
	const {
		isDarkMode,
		enable: enableDarkMode,
		disable: disableDarkMode,
	} = useDarkMode()
	return (
		<NavContainer>
			<NavMain>
				<VerticalStack>
					<Link to="/" className="homelink">
						<span className={text['display/5xl']}>
							<PsycheLogo className={c(psycheLogo, iconClass)} />
							<span>nous psyche</span>
						</span>
					</Link>
					<span className={text['body/sm/medium']}>
						DISTRIBUTED INTELLIGENCE NETWORK
						<Symbol02 className={c(symbol02, iconClass)} />
					</span>
					<Chip className={text['aux/xs/semibold']}>TESTNET</Chip>
				</VerticalStack>
				<VerticalStack className={c(text['body/sm/medium'], 'blurb')}>
					<div>Cooperative training over&#8209;the&#8209;internet</div>
					<SymbolSeparatedItems>
						<a
							href="https://github.com/PsycheFoundation/psyche"
							title="psyche's source code"
						>
							github
						</a>
						<a
							href="https://forum.psyche.network/"
							title="discuss psyche's code & propose new models"
						>
							forum
						</a>
						<a
							href="https://nousresearch.com/nous-psyche/"
							title="read the psyche annoucement"
						>
							about psyche
						</a>
						<a
							href="https://docs.psyche.network/"
							title="read about how psyche works"
						>
							docs
						</a>
					</SymbolSeparatedItems>
				</VerticalStack>
				<VerticalStack className="buttons">
					<Button
						style="theme"
						icon={{ side: 'left', svg: Sun }}
						pressed={!isDarkMode}
						onClick={disableDarkMode}
					>
						Light
					</Button>
					<Button
						style="theme"
						icon={{ side: 'left', svg: Moon }}
						pressed={isDarkMode}
						onClick={enableDarkMode}
					>
						Dark
					</Button>
				</VerticalStack>
			</NavMain>
			<CornerFleur className={`${svgFillCurrentColor} ${cornerFleur}`} />
		</NavContainer>
	)
}
