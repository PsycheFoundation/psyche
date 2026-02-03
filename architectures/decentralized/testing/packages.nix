{
  psycheLib,
  pkgs,
  inputs,
  ...
}:

let
  system = pkgs.stdenv.hostPlatform.system;
in
psycheLib.buildRustPackage {
  cratePath = ./.;
  # all tests need solana CLI and just
  buildInputs.test = [
    inputs.solana-pkgs.packages.${system}.solana
    pkgs.just
  ];
}
