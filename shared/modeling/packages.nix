{ buildRustPackage, ... }:

buildRustPackage {
  needsGpu = true;
  needsPython = "optional";
  cratePath = ./.;
}
