import {
	idlInstructionDecode,
	IdlProgram,
	idlProgramGuessInstruction,
	Instruction,
	JsonValue,
	Pubkey,
	RpcHttp,
	rpcHttpWaitForTransaction,
	RpcTransactionCallStack,
	RpcTransactionExecution,
	Signature,
} from 'solana-kiss'
import { IndexingCheckpoint } from './IndexingCheckpoint'
import { indexingSignaturesLoop } from './IndexingSignatures'

export type IndexingInstructionHandler = (indexed: {
	blockTime: Date | undefined
	instructionOrdinal: bigint
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
}) => Promise<void>

export async function indexingInstructionsLoop(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	programIdl: IdlProgram,
	onInstruction: IndexingInstructionHandler,
	onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>
): Promise<void> {
	await indexingSignaturesLoop(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		async (updatedCheckpoint, transactionsInfos) => {
			const promises = new Array()
			for (let index = 0; index < transactionsInfos.length; index++) {
				const { transactionId, transactionOrdinal } = transactionsInfos[index]!
				promises.push(
					indexingSignatureInstructions(
						rpcHttp,
						programAddress,
						programIdl,
						transactionId,
						transactionOrdinal,
						onInstruction
					)
				)
			}
			await Promise.all(promises)
			console.log(
				'>',
				new Date().toISOString(),
				programAddress,
				transactionsInfos.length
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
	programIdl: IdlProgram,
	transactionId: Signature,
	transactionOrdrinal: bigint,
	onInstruction: IndexingInstructionHandler
): Promise<void> {
	try {
		const { transactionExecution, transactionCallStack } =
			await rpcHttpWaitForTransaction(rpcHttp, transactionId, 1000)
		await indexingTransactionInstructions(
			programAddress,
			programIdl,
			transactionOrdrinal,
			transactionExecution,
			transactionCallStack,
			onInstruction
		)
	} catch (error) {
		console.error('Failed to index signature', transactionId, error)
	}
}

async function indexingTransactionInstructions(
	programAddress: Pubkey,
	programIdl: IdlProgram,
	transactionOrdinal: bigint,
	transactionExecution: RpcTransactionExecution,
	transactionCallStack: RpcTransactionCallStack | undefined,
	onInstruction: IndexingInstructionHandler
): Promise<void> {
	if (transactionExecution.error !== null) {
		return
	}
	if (transactionCallStack === undefined) {
		return
	}
	const instructionOrdinal = transactionOrdinal
	indexingInvocationsInstructions(
		transactionCallStack,
		instructionOrdinal,
		async (instructionOrdinal, instruction) => {
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
			await onInstruction({
				blockTime: transactionExecution.blockInfo.time,
				instructionOrdinal,
				instructionName: instructionIdl.name,
				instructionAddresses,
				instructionPayload,
			})
		}
	)
}

async function indexingInvocationsInstructions(
	transactionCallStack: RpcTransactionCallStack,
	instructionOrdinal: bigint,
	visitor: (
		instructionOrdinal: bigint,
		instruction: Instruction
	) => Promise<void>
): Promise<bigint> {
	for (
		let transactionCallIndex = transactionCallStack.length - 1;
		transactionCallIndex >= 0;
		transactionCallIndex--
	) {
		const transactionCallEnum = transactionCallStack[transactionCallIndex]!
		if (!('invoke' in transactionCallEnum)) {
			continue
		}
		try {
			await visitor(instructionOrdinal, transactionCallEnum.invoke.instruction)
		} catch (error) {
			console.error('Failed to process instruction', error)
		}
		instructionOrdinal += 1n
		instructionOrdinal = await indexingInvocationsInstructions(
			transactionCallEnum.invoke.callStack,
			instructionOrdinal,
			visitor
		)
	}
	return instructionOrdinal
}
