import { Pubkey, Solana, TransactionFlow } from "solana-kiss";
import { IndexerInstruction } from "./IndexerTypes";

export async function indexerParse(
  solana: Solana,
  programAddress: Pubkey,
  blockTime: Date | undefined,
  transactionOrdinal: bigint,
  transactionFlow: TransactionFlow,
): Promise<Array<IndexerInstruction>> {
  const resolvedInstructions = new Array<IndexerInstruction>();
  await visitInstructions(
    solana,
    programAddress,
    blockTime,
    transactionOrdinal * 1000n,
    transactionFlow,
    resolvedInstructions,
  );
  return resolvedInstructions;
}

async function visitInstructions(
  solana: Solana,
  programAddress: Pubkey,
  blockTime: Date | undefined,
  newerInstructionOrdinal: bigint,
  transactionFlow: TransactionFlow,
  resolvedInstructions: Array<IndexerInstruction>,
): Promise<bigint> {
  for (
    let transactionCallIndex = transactionFlow.length - 1;
    transactionCallIndex >= 0;
    transactionCallIndex--
  ) {
    const transactionCall = transactionFlow[transactionCallIndex]!;
    if (!("invocation" in transactionCall)) {
      continue;
    }
    const transactionInvocation = transactionCall.invocation;
    const instructionRequest = transactionInvocation.instructionRequest;
    if (instructionRequest.programAddress === programAddress) {
      try {
        const { instructionIdl, instructionAddresses, instructionPayload } =
          await solana.inferAndDecodeInstruction(instructionRequest);
        resolvedInstructions.push({
          blockTime: blockTime ?? null,
          instructionOrdinal: newerInstructionOrdinal,
          instructionName: instructionIdl.name,
          instructionAddresses,
          instructionPayload,
        });
      } catch (error) {
        console.error("Failed to parse instruction", programAddress, error);
      }
    }
    newerInstructionOrdinal += 1n;
    newerInstructionOrdinal = await visitInstructions(
      solana,
      programAddress,
      blockTime,
      newerInstructionOrdinal,
      transactionInvocation.flow,
      resolvedInstructions,
    );
  }
  return newerInstructionOrdinal;
}
