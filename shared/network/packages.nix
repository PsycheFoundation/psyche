{ psycheLib, ... }:

# Library package - examples will be auto-discovered and built
psycheLib.buildRustPackage {
  needsPython = "optional";
  cratePath = ./.;
}
