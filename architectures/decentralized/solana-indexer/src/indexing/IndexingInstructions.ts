import {
	idlInstructionDecode,
	IdlProgram,
	idlProgramGuessInstruction,
	Instruction,
	JsonValue,
	Pubkey,
	RpcHttp,
	rpcHttpWaitForTransaction,
	RpcTransactionExecution,
	RpcTransactionInvocation,
	Signature,
} from 'solana-kiss'
import { IndexingCheckpoint } from './IndexingCheckpoint'
import { indexingSignaturesLoop } from './IndexingSignatures'

export async function indexingInstructionsLoop(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	programIdl: IdlProgram,
	onInstruction: (
		instructionName: string,
		instructionAddresses: Map<string, Pubkey>,
		instructionPayload: JsonValue,
		context: {
			ordering: bigint
			instruction: Instruction
			transaction: {
				execution: RpcTransactionExecution
				invocations: Array<RpcTransactionInvocation> | undefined
			}
		}
	) => Promise<void>,
	onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>
): Promise<void> {
	await indexingSignaturesLoop(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		async (foundHistory, updatedCheckpoint) => {
			const promises = new Array()
			for (let index = 0; index < foundHistory.length; index++) {
				const { signature, ordering } = foundHistory[index]!
				promises.push(
					indexingSignatureInstructions(
						rpcHttp,
						programAddress,
						signature,
						ordering,
						programIdl,
						onInstruction
					)
				)
			}
			await Promise.all(promises)
			console.log(
				'>',
				new Date().toISOString(),
				programAddress,
				foundHistory.length
			)
			try {
				await onCheckpoint(updatedCheckpoint)
			} catch (error) {
				console.error('Failed to save checkpoint', error)
			}
		}
	)
}

async function indexingSignatureInstructions(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	signature: Signature,
	ordering: bigint,
	programIdl: IdlProgram,
	onInstruction: (
		instructionName: string,
		instructionAddresses: Map<string, Pubkey>,
		instructionPayload: JsonValue,
		context: {
			ordering: bigint
			instruction: Instruction
			transaction: {
				execution: RpcTransactionExecution
				invocations: Array<RpcTransactionInvocation> | undefined
			}
		}
	) => Promise<void>
): Promise<void> {
	try {
		const transaction = await rpcHttpWaitForTransaction(
			rpcHttp,
			signature,
			1000
		)
		await indexingTransactionInstructions(
			programAddress,
			programIdl,
			transaction,
			ordering,
			onInstruction
		)
	} catch (error) {
		console.error('Failed to index signature', signature, error)
	}
}

async function indexingTransactionInstructions(
	programAddress: Pubkey,
	programIdl: IdlProgram,
	transaction: {
		execution: RpcTransactionExecution
		invocations: Array<RpcTransactionInvocation> | undefined
	},
	ordering: bigint,
	onInstruction: (
		instructionName: string,
		instructionAddresses: Map<string, Pubkey>,
		instructionPayload: JsonValue,
		context: {
			ordering: bigint
			transaction: {
				execution: RpcTransactionExecution
				invocations: Array<RpcTransactionInvocation> | undefined
			}
			instruction: Instruction
		}
	) => Promise<void>
): Promise<void> {
	if (transaction.error !== null) {
		return
	}
	indexingInvocationsInstructions(
		transaction.invocations,
		ordering,
		async (instruction, ordering) => {
			if (instruction.programAddress !== programAddress) {
				return
			}
			const instructionIdl = idlProgramGuessInstruction(programIdl, instruction)
			if (instructionIdl === undefined) {
				return
			}
			const { instructionAddresses, instructionPayload } = idlInstructionDecode(
				instructionIdl,
				instruction
			)
			await onInstruction(
				instructionIdl.name,
				instructionAddresses,
				instructionPayload,
				{ ordering, transaction, instruction }
			)
		}
	)
}

async function indexingInvocationsInstructions(
	invocations: Array<TransactionInvocation>,
	ordering: bigint,
	visitor: (instruction: Instruction, ordering: bigint) => Promise<void>
): Promise<bigint> {
	for (
		let invocationIndex = invocations.length - 1;
		invocationIndex >= 0;
		invocationIndex--
	) {
		const invocation = invocations[invocationIndex]!
		try {
			await visitor(invocation.instruction, ordering)
		} catch (error) {
			console.error('Failed to process instruction', error)
		}
		ordering += 1n
		ordering = await indexingInvocationsInstructions(
			invocation.invocations,
			ordering,
			visitor
		)
	}
	return ordering
}
