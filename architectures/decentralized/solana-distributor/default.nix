{ psycheLib }:
psycheLib.buildSolanaIdl {
  src = psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-distributor";
  programName = "solana-distributor";
  keypair = ../local-dev-keypair.json;
}
