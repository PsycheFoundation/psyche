{ buildRustPackage, ... }:

buildRustPackage {
  needsPython = "optional";
  cratePath = ./.;
}
