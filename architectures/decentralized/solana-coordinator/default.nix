{ psycheLib }:
psycheLib.buildSolanaIdl {
  src = psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-coordinator";
  programName = "solana-coordinator";
  keypair = ../local-dev-keypair.json;
}
