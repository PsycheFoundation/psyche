{ psycheLib, ... }:

psycheLib.buildRustPackage {
  needsPython = "optional";
  needsGpu = true;
  cratePath = ./.;
}
