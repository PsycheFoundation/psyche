{ psycheLib }:
psycheLib.buildSolanaProgram {
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-coordinator";
  programName = "solana-coordinator";
  keypair = ./target/deploy/psyche_solana_coordinator-keypair.json;
}
