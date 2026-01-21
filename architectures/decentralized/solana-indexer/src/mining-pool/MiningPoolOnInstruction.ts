import {
  jsonCodecBigInt,
  jsonDecoderObjectToObject,
  Pubkey,
} from "solana-kiss";
import { IndexerInstruction } from "../indexer/IndexerTypes";
import { utilsBigintArraySortAscending } from "../utils";
import { MiningPoolDataStore } from "./MiningPoolDataStore";

export function miningPoolOnInstruction(
  dataStore: MiningPoolDataStore,
  instruction: IndexerInstruction,
) {
  const poolAddress = instruction.instructionAddresses["pool"];
  if (poolAddress === undefined) {
    throw new Error(
      "MiningPool: Instruction: PoolExtract: Missing pool address",
    );
  }
  const signerAddress =
    instruction.instructionAddresses["authority"] ??
    instruction.instructionAddresses["user"];
  if (signerAddress === undefined) {
    throw new Error("MiningPool: Instruction: Could not find signer address");
  }
  const processors = processorsByInstructionName.get(
    instruction.instructionName,
  );
  if (processors !== undefined) {
    for (const processor of processors) {
      processor(dataStore, {
        poolAddress,
        signerAddress,
        instruction: instruction,
      });
    }
  } else {
    console.warn(
      "MiningPool: Unknown instruction:",
      instruction.instructionName,
    );
  }
  const poolAnalysis = dataStore.getPoolAnalysis(poolAddress);
  if (instruction.instructionOrdinal > poolAnalysis.latestKnownChangeOrdinal) {
    poolAnalysis.latestKnownChangeOrdinal = instruction.instructionOrdinal;
  }
}

const processorsByInstructionName = new Map([
  ["pool_create", [processAdminAction]],
  ["pool_update", [processAdminAction]],
  ["pool_extract", [processAdminAction, processPoolExtract]],
  ["pool_claimable", [processAdminAction]],
  ["lender_create", []],
  ["lender_deposit", [processLenderDeposit]],
  ["lender_claim", [processLenderClaim]],
]);

type ProcessingContext = {
  poolAddress: Pubkey;
  signerAddress: Pubkey;
  instruction: IndexerInstruction;
};

async function processAdminAction(
  dataStore: MiningPoolDataStore,
  context: ProcessingContext,
): Promise<void> {
  const poolAnalysis = dataStore.getPoolAnalysis(context.poolAddress);
  poolAnalysis.importantHistory.push(context.instruction);
  utilsBigintArraySortAscending(
    poolAnalysis.importantHistory,
    (importantAction) => importantAction.instructionOrdinal,
  );
  poolAnalysis.importantHistory.reverse();
}

async function processPoolExtract(
  dataStore: MiningPoolDataStore,
  context: ProcessingContext,
): Promise<void> {
  const poolExtractPayload = poolExtractJsonDecoder(
    context.instruction.instructionPayload,
  );
  const poolAnalysis = dataStore.getPoolAnalysis(context.poolAddress);
  poolAnalysis.totalExtractCollateralAmount +=
    poolExtractPayload.params.collateralAmount;
}

async function processLenderDeposit(
  dataStore: MiningPoolDataStore,
  context: ProcessingContext,
): Promise<void> {
  const lenderDepositPayload = lenderDepositJsonDecoder(
    context.instruction.instructionPayload,
  );
  const poolAnalysis = dataStore.getPoolAnalysis(context.poolAddress);
  poolAnalysis.depositCollateralAmountPerUser.set(
    context.signerAddress,
    (poolAnalysis.depositCollateralAmountPerUser.get(context.signerAddress) ??
      0n) + lenderDepositPayload.params.collateralAmount,
  );
  poolAnalysis.totalDepositCollateralAmount +=
    lenderDepositPayload.params.collateralAmount;
}

async function processLenderClaim(
  dataStore: MiningPoolDataStore,
  context: ProcessingContext,
): Promise<void> {
  const lenderClaimPayload = lenderClaimJsonDecoder(
    context.instruction.instructionPayload,
  );
  const poolAnalysis = dataStore.getPoolAnalysis(context.poolAddress);
  poolAnalysis.claimRedeemableAmountPerUser.set(
    context.signerAddress,
    (poolAnalysis.claimRedeemableAmountPerUser.get(context.signerAddress) ??
      0n) + lenderClaimPayload.params.redeemableAmount,
  );
  poolAnalysis.totalClaimRedeemableAmount +=
    lenderClaimPayload.params.redeemableAmount;
}

const poolExtractJsonDecoder = jsonDecoderObjectToObject({
  params: jsonDecoderObjectToObject({
    collateralAmount: jsonCodecBigInt.decoder,
  }),
});

const lenderDepositJsonDecoder = jsonDecoderObjectToObject({
  params: jsonDecoderObjectToObject({
    collateralAmount: jsonCodecBigInt.decoder,
  }),
});

const lenderClaimJsonDecoder = jsonDecoderObjectToObject({
  params: jsonDecoderObjectToObject({
    redeemableAmount: jsonCodecBigInt.decoder,
  }),
});
