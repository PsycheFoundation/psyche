{ psycheLib }:
psycheLib.buildSolanaIdl {
  src = psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-coordinator";
  programName = "solana-coordinator";
  keypair = ./target/deploy/psyche_solana_coordinator-keypair.json;
}
