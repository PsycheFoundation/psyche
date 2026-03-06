{ psycheLib }:
psycheLib.buildSolanaProgram {
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-authorizer";
  programName = "solana-authorizer";
  keypair = ./target/deploy/psyche_solana_authorizer-keypair.json;
}
