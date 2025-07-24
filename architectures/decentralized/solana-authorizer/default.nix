{ psycheLib }:
psycheLib.buildSolana {
  src = psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-authorizer";
  programName = "solana-authorizer";
  keypair = ../local-dev-keypair.json;
}
