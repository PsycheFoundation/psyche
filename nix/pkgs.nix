{
  pkgs,
  lib ? pkgs.lib,
  inputs,
}:
lib.makeScope pkgs.newScope (
  self:
  let
    rustPackageNames = [
      "psyche-solana-client"
      "psyche-centralized-client"
      "psyche-centralized-server"
      "psyche-centralized-local-testnet"
      "expand-distro"
      "preview-lr"
    ];

    rustPackages = lib.genAttrs rustPackageNames self.psycheLib.buildRustPackage;

    nixglhostRustPackages = lib.listToAttrs (
      map (
        name: lib.nameValuePair "${name}-nixglhost" (self.psycheLib.useHostGpuDrivers rustPackages.${name})
      ) rustPackageNames
    );

    psychePackages =
      {
        psyche-website-wasm = self.callPackage ../website/wasm { };
        psyche-website-shared = self.callPackage ../website/shared { };

        psyche-deserialize-zerocopy-wasm = self.psycheLib.buildRustWasmTsPackage "psyche-deserialize-zerocopy-wasm";

        solana-coordinator-idl = self.callPackage ../architectures/decentralized/solana-coordinator { };
        solana-mining-pool-idl = self.callPackage ../architectures/decentralized/solana-mining-pool { };

        psyche-book = pkgs.callPackage ../psyche-book { inherit rustPackages rustPackageNames; };
      }
      // rustPackages
      // nixglhostRustPackages;
  in
  {
    psycheLib = import ./lib.nix {
      inherit pkgs inputs;
    };

    inherit psychePackages;
  }
  // lib.mapAttrs (_: lib.id) psychePackages
)
