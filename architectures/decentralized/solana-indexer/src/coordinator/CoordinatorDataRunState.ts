import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecInteger,
	jsonCodecObject,
	jsonCodecPubkey,
	jsonCodecString,
	Pubkey,
} from 'solana-kiss'

export interface CoordinatorDataRunState {
	runId: string
	name: string
	description: string
	status: string
	epochClients: Array<{ signer: Pubkey; state: string }>
	nonce: bigint
}

export const coordinatorDataRunStateJsonCodec: JsonCodec<CoordinatorDataRunState> =
	jsonCodecObject({
		runId: jsonCodecString,
		name: jsonCodecString,
		description: jsonCodecString,
		status: jsonCodecString,
		epochClients: jsonCodecArray(
			jsonCodecObject({
				signer: jsonCodecPubkey,
				state: jsonCodecString,
			})
		),
		nonce: jsonCodecInteger,
	})
