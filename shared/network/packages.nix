{ psycheLib, ... }:

psycheLib.buildRustPackage {
  needsPython = "optional";
  cratePath = ./.;
}
