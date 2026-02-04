{ psycheLib, ... }:

psycheLib.buildRustPackage {
  needsGpu = true;
  needsPython = "optional";
  cratePath = ./.;
}
