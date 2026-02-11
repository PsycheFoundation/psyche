{ buildRustPackage, ... }:

buildRustPackage {
  needsPython = "optional";
  needsGpu = true;
  cratePath = ./.;
}
