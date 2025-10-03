import { JsonValue, Pubkey, Signature } from "solana-kiss-data";
import { IdlProgram } from "solana-kiss-idl";
import { RpcHttp, rpcHttpGetTransaction, Transaction } from "solana-kiss-rpc";
import { Immutable } from "../utils";
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
      instructionIndex: number;
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
        const execution = await rpcHttpGetTransaction(rpcHttp, signature);
        if (execution === undefined) {
          return;
        }
        if (execution.error !== null) {
          return;
        }
        const source = { signature, execution, instructionIndex: -1 };
        for (
          let instructionIndex = 0;
          instructionIndex < execution.instructions.length;
          instructionIndex++
        ) {
          source.instructionIndex = instructionIndex;
          const instruction = execution.instructions[instructionIndex]!;
          try {
            if (!instruction.programId.equals(programAddress)) {
              continue;
            }
            const instructionIdl = programIdl.guessInstruction(
              instruction.data,
            );
            if (!instructionIdl) {
              continue;
            }
            const { instructionAddresses, instructionPayload } =
              instructionIdl.decode(instruction);
            await onInstruction(
              instructionIdl.name,
              instructionAddresses,
              instructionPayload,
              ordering * 1000n + BigInt(instructionIndex),
              source,
            );
          } catch (error) {
            console.error("Failed to process instruction content", error);
          }
        }
      } catch (error) {
        console.error("Failed to get execution", signature, "ERR", error);
      }
    },
    onCheckpoint,
  );
}
