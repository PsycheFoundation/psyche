{ psycheLib, ... }:

psycheLib.buildRustPackage {
  needsPython = true;
  needsGpu = true;
  cratePath = ./.;
  # vllm doesn't build on macos
  supportedSystems = [
    "x86_64-linux"
    "aarch64-linux"
  ];
}
