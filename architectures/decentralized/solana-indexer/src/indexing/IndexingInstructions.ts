import { Immutable, Instruction, JsonValue, Pubkey } from "solana-kiss-data";
import {
  idlInstructionDecode,
  IdlProgram,
  idlProgramGuessInstruction,
} from "solana-kiss-idl";
import {
  Invocation,
  RpcHttp,
  rpcHttpWaitForTransaction,
  Transaction,
} from "solana-kiss-rpc";
import { IndexingCheckpoint } from "./IndexingCheckpoint";
import { indexingSignaturesLoop } from "./IndexingSignatures";

export async function indexingInstructionsLoop(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  startingCheckpoint: IndexingCheckpoint,
  programIdl: IdlProgram,
  onInstruction: (
    instructionName: string,
    instructionAddresses: Map<string, Pubkey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    source: Immutable<{
      transaction: Transaction;
      instruction: Instruction;
    }>,
  ) => Promise<void>,
  onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>,
): Promise<void> {
  await indexingSignaturesLoop(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    async (foundHistory, updatedCheckpoint) => {
      const promises = new Array();
      for (let index = 0; index < foundHistory.length; index++) {
        const { signature, ordering } = foundHistory[index]!;
        promises.push(
          indexingSignatureInstructions(
            rpcHttp,
            programAddress,
            signature,
            ordering,
            programIdl,
            onInstruction,
          ),
        );
      }
      await Promise.all(promises);
      try {
        await onCheckpoint(updatedCheckpoint);
      } catch (error) {
        console.error("Failed to save checkpoint", error);
      }
    },
  );
}

async function indexingSignatureInstructions(
  rpcHttp: RpcHttp,
  programAddress: Pubkey,
  signature: string,
  ordering: bigint,
  programIdl: IdlProgram,
  onInstruction: (
    instructionName: string,
    instructionAddresses: Map<string, Pubkey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    source: Immutable<{
      transaction: Transaction;
      instruction: Instruction;
    }>,
  ) => Promise<void>,
): Promise<void> {
  try {
    const transaction = await rpcHttpWaitForTransaction(
      rpcHttp,
      signature,
      1000,
    );
    await indexingTransactionInstructions(
      programAddress,
      programIdl,
      transaction,
      ordering,
      onInstruction,
    );
  } catch (error) {
    console.error("Failed to index signature", signature, error);
  }
}

async function indexingTransactionInstructions(
  programAddress: Pubkey,
  programIdl: IdlProgram,
  transaction: Transaction,
  ordering: bigint,
  onInstruction: (
    instructionName: string,
    instructionAddresses: Map<string, Pubkey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    source: Immutable<{
      transaction: Transaction;
      instruction: Instruction;
    }>,
  ) => Promise<void>,
): Promise<void> {
  if (transaction.error !== null) {
    return;
  }
  indexingInvocationsInstructions(
    transaction.invocations,
    ordering,
    async (instruction, ordering) => {
      if (instruction.programAddress !== programAddress) {
        return;
      }
      const instructionIdl = idlProgramGuessInstruction(
        programIdl,
        instruction,
      );
      if (instructionIdl === undefined) {
        return;
      }
      const { instructionAddresses, instructionPayload } = idlInstructionDecode(
        instructionIdl,
        instruction,
      );
      await onInstruction(
        instructionIdl.name,
        instructionAddresses,
        instructionPayload,
        ordering,
        { transaction, instruction },
      );
    },
  );
}

async function indexingInvocationsInstructions(
  invocations: Array<Invocation>,
  ordering: bigint,
  visitor: (instruction: Instruction, ordering: bigint) => Promise<void>,
): Promise<bigint> {
  for (
    let invocationIndex = invocations.length - 1;
    invocationIndex >= 0;
    invocationIndex--
  ) {
    const invocation = invocations[invocationIndex]!;
    try {
      await visitor(invocation.instruction, ordering);
    } catch (error) {
      console.error("Failed to process instruction", error);
    }
    ordering += 1n;
    ordering = await indexingInvocationsInstructions(
      invocation.invocations,
      ordering,
      visitor,
    );
  }
  return ordering;
}
