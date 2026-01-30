import { CoordinatorRunAnalysis } from "psyche-indexer-codecs";
import {
  jsonCodecBigInt,
  jsonCodecNumber,
  jsonCodecPubkey,
  jsonCodecString,
  jsonDecoderObjectToObject,
  Pubkey,
  Solana,
} from "solana-kiss";
import {
  jsonDecoderRustClientId,
  jsonDecoderRustFixedArray,
  jsonDecoderRustFixedString,
} from "../json";

export async function coordinatorRunOnchainFetch(
  solana: Solana,
  runAddress: Pubkey,
  runAnalysis: CoordinatorRunAnalysis,
) {
  if (
    runAnalysis.latestUpdateFetchOrdinal ===
    runAnalysis.latestKnownChangeOrdinal
  ) {
    return;
  }
  runAnalysis.latestUpdateFetchOrdinal = runAnalysis.latestKnownChangeOrdinal;
  try {
    await fetchAndUpdateOnchainState(solana, runAddress, runAnalysis);
  } catch (error) {
    console.error("Failed to refresh run state", runAddress, error);
  }
}

async function fetchAndUpdateOnchainState(
  solana: Solana,
  runAddress: Pubkey,
  runAnalysis: CoordinatorRunAnalysis,
) {
  const { accountState: runInstanceState } =
    await solana.getAndInferAndDecodeAccount(runAddress);
  const runInstanceParsed = runInstanceJsonDecoder(runInstanceState);
  const runAccountAddress = runInstanceParsed.coordinatorAccount;
  const { accountState: runAccountState } =
    await solana.getAndInferAndDecodeAccount(runAccountAddress);
  const runAccountParsed = runAccountJsonDecoder(runAccountState);
  runAnalysis.latestOnchainSnapshot = {
    parsed: {
      runId: runInstanceParsed.runId,
      mainAuthority: runInstanceParsed.mainAuthority,
      joinAuthority: runInstanceParsed.joinAuthority,
      name: runAccountParsed.state.metadata.name,
      description: runAccountParsed.state.metadata.description,
      status: runAccountParsed.state.coordinator.runState,
      numParameters: runAccountParsed.state.metadata.numParameters,
      joinedClients: runAccountParsed.state.clientsState.clients.map(
        (client) => ({
          signer: client.id.signer,
          earned: client.earned,
          slashed: client.slashed,
        }),
      ),
      epochClients: runAccountParsed.state.coordinator.epochState.clients.map(
        (client) => ({ signer: client.id.signer, state: client.state }),
      ),
      progress: runAccountParsed.state.coordinator.progress,
    },
    native: {
      coordinatorInstance: runInstanceState,
      coordinatorAccount: runAccountState,
    },
    updatedAt: new Date(),
  };
}

const runInstanceJsonDecoder = jsonDecoderObjectToObject({
  bump: jsonCodecNumber.decoder,
  mainAuthority: jsonCodecPubkey.decoder,
  joinAuthority: jsonCodecPubkey.decoder,
  coordinatorAccount: jsonCodecPubkey.decoder,
  runId: jsonCodecString.decoder,
});

const runAccountJsonDecoder = jsonDecoderObjectToObject({
  nonce: jsonCodecBigInt.decoder,
  state: jsonDecoderObjectToObject({
    metadata: jsonDecoderObjectToObject({
      name: jsonDecoderRustFixedString,
      description: jsonDecoderRustFixedString,
      numParameters: jsonCodecBigInt.decoder,
      vocabSize: jsonCodecBigInt.decoder,
    }),
    coordinator: jsonDecoderObjectToObject({
      runId: jsonDecoderRustFixedString,
      runState: jsonCodecString.decoder,
      /*
      model: jsonCodecValue.decoder,
      config: jsonDecoderObjectToObject({
        warmupTime: jsonCodecBigInt.decoder,
        cooldownTime: jsonCodecBigInt.decoder,
        maxRoundTrainTime: jsonCodecBigInt.decoder,
        roundWitnessTime: jsonCodecBigInt.decoder,
        globalBatchSizeWarmupTokens: jsonCodecBigInt.decoder,
        roundsPerEpoch: jsonCodecNumber.decoder,
        totalSteps: jsonCodecNumber.decoder,
        initMinClients: jsonCodecNumber.decoder,
        minClients: jsonCodecNumber.decoder,
        witnessNodes: jsonCodecNumber.decoder,
        globalBatchSizeStart: jsonCodecNumber.decoder,
        globalBatchSizeEnd: jsonCodecNumber.decoder,
        verificationPercent: jsonCodecNumber.decoder,
      }),
      */
      progress: jsonDecoderObjectToObject({
        epoch: jsonCodecNumber.decoder,
        step: jsonCodecNumber.decoder,
        epochStartDataIndex: jsonCodecBigInt.decoder,
      }),
      epochState: jsonDecoderObjectToObject({
        // rounds: jsonCodecValue.decoder,
        clients: jsonDecoderRustFixedArray(
          jsonDecoderObjectToObject({
            id: jsonDecoderRustClientId,
            // exitedHeight: jsonCodecNumber.decoder,
            state: jsonCodecString.decoder,
          }),
        ),
        /*
        exitedClients: jsonDecoderRustFixedArray(
          jsonDecoderObjectToObject({
            id: jsonDecoderRustClientId,
            exitedHeight: jsonCodecNumber.decoder,
            state: jsonCodecString.decoder,
          }),
        ),
        roundsHead: jsonCodecNumber.decoder,
        startStep: jsonCodecNumber.decoder,
        firstRound: jsonDecoderRustSmallBoolean,
        checkpointed: jsonDecoderRustSmallBoolean,
        coldStartEpoch: jsonDecoderRustSmallBoolean,
        */
      }),
      // runStateStartUnixTimestamp: jsonCodecBigInt.decoder,
      // pendingPause: jsonDecoderRustSmallBoolean,
    }),
    clientsState: jsonDecoderObjectToObject({
      nextActive: jsonCodecBigInt.decoder,
      clients: jsonDecoderRustFixedArray(
        jsonDecoderObjectToObject({
          active: jsonCodecBigInt.decoder,
          earned: jsonCodecBigInt.decoder,
          slashed: jsonCodecBigInt.decoder,
          id: jsonDecoderRustClientId,
        }),
      ),
      /*
      currentEpochRates: jsonDecoderObjectToObject({
        earningRate: jsonCodecBigInt.decoder,
        slashingRate: jsonCodecBigInt.decoder,
      }),
      futureEpochRates: jsonDecoderObjectToObject({
        earningRate: jsonCodecBigInt.decoder,
        slashingRate: jsonCodecBigInt.decoder,
      }),
      */
    }),
    //isWarmupFirstTick: jsonDecoderRustSmallBoolean,
    //isTrainingFirstTick: jsonDecoderRustSmallBoolean,
  }),
});
