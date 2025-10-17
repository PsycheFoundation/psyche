import {
	JsonCodec,
	jsonCodecArray,
	jsonCodecInteger,
	jsonCodecNumber,
	jsonCodecObject,
	jsonCodecPubkey,
	jsonCodecRaw,
	jsonCodecString,
	JsonValue,
	Pubkey,
} from 'solana-kiss'

export interface CoordinatorDataRunState {
	runId: string
	mainAuthority: Pubkey
	joinAuthority: Pubkey
	name: string
	description: string
	status: string
	model: JsonValue
	epochClients: Array<{ signer: Pubkey; state: string }>
	progress: {
		epoch: number
		step: number
	}
	nonce: bigint
}

export const coordinatorDataRunStateJsonCodec: JsonCodec<CoordinatorDataRunState> =
	jsonCodecObject({
		runId: jsonCodecString,
		mainAuthority: jsonCodecPubkey,
		joinAuthority: jsonCodecPubkey,
		name: jsonCodecString,
		description: jsonCodecString,
		status: jsonCodecString,
		model: jsonCodecRaw,
		epochClients: jsonCodecArray(
			jsonCodecObject({
				signer: jsonCodecPubkey,
				state: jsonCodecString,
			})
		),
		progress: jsonCodecObject({
			epoch: jsonCodecNumber,
			step: jsonCodecNumber,
		}),
		nonce: jsonCodecInteger,
	})
