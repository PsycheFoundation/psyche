{ psycheLib }:
psycheLib.buildSolanaProgram {
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-coordinator";
  programName = "solana-coordinator";
  keypair = ../local-dev-keypair.json;
}
