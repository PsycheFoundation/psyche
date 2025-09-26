import { PublicKey, TransactionSignature } from "@solana/web3.js";
import {
  ToolboxEndpoint,
  ToolboxEndpointExecution,
  ToolboxIdlProgram,
} from "solana_toolbox_web3";
import { JsonValue } from "../json";
import { Immutable } from "../utils";
import { IndexingCheckpoint } from "./IndexingCheckpoint";
import { indexingSignaturesLoop } from "./IndexingSignatures";

export async function indexingInstructionsLoop(
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  idlProgram: ToolboxIdlProgram,
  onInstruction: (
    instructionName: string,
    instructionAddresses: Map<string, PublicKey>,
    instructionPayload: JsonValue,
    ordering: bigint,
    source: Immutable<{
      signature: TransactionSignature;
      execution: ToolboxEndpointExecution;
      instructionIndex: number;
    }>,
  ) => Promise<void>,
  onCheckpoint: (indexedCheckpoint: IndexingCheckpoint) => Promise<void>,
): Promise<void> {
  await indexingSignaturesLoop(
    endpoint,
    programAddress,
    startingCheckpoint,
    async (signature: TransactionSignature, ordering: bigint) => {
      try {
        const execution = await endpoint.getExecution(signature);
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
            const idlInstruction = idlProgram.guessInstruction(
              instruction.data,
            );
            if (!idlInstruction) {
              continue;
            }
            const { instructionAddresses, instructionPayload } =
              idlInstruction.decode(instruction);
            await onInstruction(
              idlInstruction.name,
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
