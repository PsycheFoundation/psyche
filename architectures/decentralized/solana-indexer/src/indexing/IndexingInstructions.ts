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
	Signature,
} from 'solana-kiss'
import { IndexingCheckpoint } from './IndexingCheckpoint'
import { indexingTransactionsIds } from './IndexingTransactions'

export type IndexingInstructionHandler = (indexed: {
	blockTime: Date | undefined
	instructionOrdinal: bigint
	instructionName: string
	instructionAddresses: Map<string, Pubkey>
	instructionPayload: JsonValue
}) => Promise<void>

export async function indexingInstructions(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	startingCheckpoint: IndexingCheckpoint,
	programIdl: IdlProgram,
	onInstruction: IndexingInstructionHandler,
	onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>
): Promise<void> {
	await indexingTransactionsIds(
		rpcHttp,
		programAddress,
		startingCheckpoint,
		async (updatedCheckpoint, transactionsInfos) => {
			const promises = new Array()
			for (let index = 0; index < transactionsInfos.length; index++) {
				const { transactionId, transactionOrdinal } = transactionsInfos[index]!
				promises.push(
					indexingTransaction(
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
				'indexed:',
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

async function indexingTransaction(
	rpcHttp: RpcHttp,
	programAddress: Pubkey,
	programIdl: IdlProgram,
	transactionId: Signature,
	transactionOrdinal: bigint,
	onInstruction: IndexingInstructionHandler
): Promise<void> {
	try {
		const { transactionExecution, transactionCallStack } =
			await rpcHttpWaitForTransaction(rpcHttp, transactionId, 10_000)
		if (transactionExecution.error !== null) {
			return
		}
		if (transactionCallStack === undefined) {
			return
		}
		indexingTransactionCallStack(
			transactionCallStack,
			transactionOrdinal,
			async (instructionOrdinal, instruction) => {
				if (instruction.programAddress !== programAddress) {
					return
				}
				const instructionIdl = idlProgramGuessInstruction(
					programIdl,
					instruction
				)
				if (instructionIdl === undefined) {
					return
				}
				const { instructionAddresses, instructionPayload } =
					idlInstructionDecode(instructionIdl, instruction)
				await onInstruction({
					blockTime: transactionExecution.blockInfo.time,
					instructionOrdinal,
					instructionName: instructionIdl.name,
					instructionAddresses,
					instructionPayload,
				})
			}
		)
	} catch (error) {
		console.error('Failed to index signature', transactionId, error)
	}
}

async function indexingTransactionCallStack(
	transactionCallStack: RpcTransactionCallStack,
	instructionOrdinal: bigint,
	instructionVisitor: (
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
			await instructionVisitor(
				instructionOrdinal,
				transactionCallEnum.invoke.instruction
			)
		} catch (error) {
			console.error('Failed to process instruction', error)
		}
		instructionOrdinal += 1n
		instructionOrdinal = await indexingTransactionCallStack(
			transactionCallEnum.invoke.callStack,
			instructionOrdinal,
			instructionVisitor
		)
	}
	return instructionOrdinal
}
