{ psycheLib }:
psycheLib.buildSolanaIdl {
  src = psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-mining-pool";
  programName = "solana-mining-pool";
  keypair = ./target/deploy/psyche_solana_mining_pool-keypair.json;
}
