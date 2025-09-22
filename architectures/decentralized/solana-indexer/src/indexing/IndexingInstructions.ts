import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint, ToolboxIdlService } from "solana_toolbox_web3";
import { IndexingCheckpoint } from "./IndexingCheckpoint";
import { indexingSignaturesLoop } from "./IndexingSignatures";

export async function indexingInstructionsLoop(
  endpoint: ToolboxEndpoint,
  programAddress: PublicKey,
  startingCheckpoint: IndexingCheckpoint,
  onInstruction: (
    instructionName: string,
    instructionAddresses: Map<string, PublicKey>,
    instructionPayload: any,
    ordering: bigint,
  ) => Promise<void>,
  onCheckpoint: (checkpoint: IndexingCheckpoint) => Promise<void>,
): Promise<void> {
  const TO_REMOVE: {
    ordering: bigint;
    slot: number;
    processedTime: Date | null;
  }[] = [];
  const idlProgram = await new ToolboxIdlService().getOrResolveProgram(
    endpoint,
    programAddress,
  );
  if (!idlProgram) {
    throw new Error(`Failed to resolve program IDL: ${programAddress}`);
  }
  await indexingSignaturesLoop(
    endpoint,
    programAddress,
    startingCheckpoint,
    async (signature, ordering) => {
      try {
        const execution = await endpoint.getExecution(signature);
        TO_REMOVE.push({
          ordering: ordering,
          slot: execution.slot,
          processedTime: execution.processedTime,
        });
        TO_REMOVE.sort((a, b) => {
          return -Number(a.ordering - b.ordering);
        });
        for (let i = 1; i < TO_REMOVE.length; i++) {
          const curr = TO_REMOVE[i - 1]!;
          const next = TO_REMOVE[i]!;
          if (
            curr.slot < next.slot ||
            curr.processedTime!.getTime() < next.processedTime!.getTime()
          ) {
            // TODO - to remove all this (maybe use tests instead)
            console.warn(
              "Slot ordering issue",
              curr.ordering,
              curr.slot,
              curr.processedTime!.getTime(),
              next.ordering,
              next.slot,
              next.processedTime!.getTime(),
            );
          }
        }
        for (
          let instructionIndex = 0;
          instructionIndex < execution.instructions.length;
          instructionIndex++
        ) {
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
