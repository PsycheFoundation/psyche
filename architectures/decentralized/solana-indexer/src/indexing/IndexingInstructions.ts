import {
  Immutable,
  Instruction,
  JsonValue,
  Pubkey,
  Signature,
} from "solana-kiss-data";
import {
  idlInstructionDecode,
  IdlProgram,
  idlProgramGuessInstruction,
} from "solana-kiss-idl";
import {
  Invocation,
  RpcHttp,
  rpcHttpGetTransaction,
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
      signature: Signature;
      transaction: Transaction;
      instruction: Instruction;
    }>,
  ) => Promise<void>,
  onCheckpoint: (indexedCheckpoint: IndexingCheckpoint) => Promise<void>,
): Promise<void> {
  await indexingSignaturesLoop(
    rpcHttp,
    programAddress,
    startingCheckpoint,
    async (signature: Signature, ordering: bigint) => {
      try {
        const transaction = await rpcHttpGetTransaction(rpcHttp, signature);
        if (transaction === undefined) {
          return;
        }
        if (transaction.error !== null) {
          return;
        }
        indexingInvocationsInstructions(
          transaction.invocations,
          ordering * 1000n,
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
            const { instructionAddresses, instructionPayload } =
              idlInstructionDecode(instructionIdl, instruction);
            await onInstruction(
              instructionIdl.name,
              instructionAddresses,
              instructionPayload,
              ordering,
              { signature, transaction, instruction },
            );
          },
        );
      } catch (error) {
        console.error("Failed to get execution", signature, "ERR", error);
      }
    },
    async (checkpoint) => {
      try {
        await onCheckpoint(checkpoint);
      } catch (error) {
        console.error("Failed to save checkpoint", checkpoint, "ERR", error);
      }
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
      console.error(
        "Failed to process instruction",
        invocation.instruction,
        "ERR",
        error,
      );
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
