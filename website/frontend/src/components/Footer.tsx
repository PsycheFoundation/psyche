import { styled } from '@linaria/react'
import { SymbolSeparatedItems } from './SymbolSeparatedItems.js'
import { text } from '../fonts.js'
import IrohLogo from '../assets/logos/iroh.svg?react'
import TorchLogo from '../assets/logos/pytorch.svg?react'
import HfLogo from '../assets/logos/hf.svg?react'
import { slate } from '../colors.js'
import { Link } from '@tanstack/react-router'

const Container = styled.div`
	background: var(--color-bg);
	color: ${slate[500]};
	display: flex;
	flex-wrap: wrap;
	justify-content: space-between;
	padding: 2px 16px;
	gap: 12px;

	& a {
		color: ${slate[500]};
	}
`

const PoweredBy = styled.div`
	display: flex;
	justify-content: space-between;
	align-items: center;
	gap: 12px;
	border-radius: 8px;
	a {
		display: flex;
		align-items: center;
		justify-content: center;
		svg {
			height: 1em;
			width: 100%;
		}
	}
`
const ExpandContainer = styled.div`
	flex-grow: 1;
	display: flex;
	flex-direction: column;
	.empty {
		flex-basis: 0px;
		flex-grow: 1;
	}
`

export function Footer() {
	return (
		<ExpandContainer>
			<div className="empty" />
			<Container className={text['aux/sm/medium']}>
				<div>&copy;{new Date().getFullYear()} psyche foundation</div>
				<PoweredBy>
					<span>built using libraries by</span>
					<a href="https://www.iroh.computer/">
						<IrohLogo />
					</a>
					<a href="https://pytorch.org/">
						<TorchLogo />
					</a>
					<a href="https://huggingface.co/">
						<HfLogo />
					</a>
				</PoweredBy>
				<SymbolSeparatedItems>
					<a href="https://twitter.com/psycheoperation">twitter</a>
					<a href="https://discord.gg/jqVphNsB4H">discord</a>
					<Link to="/legal">legal</Link>
					<a href="https://nousresearch.com">nous research</a>
				</SymbolSeparatedItems>
			</Container>
		</ExpandContainer>
	)
}
