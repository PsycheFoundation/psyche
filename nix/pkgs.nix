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

    rustExampleNames = [
      "bandwidth_test"
      "inference"
      "train"
    ];

    rustPackages = lib.mapAttrs (_: lib.id) (
      lib.genAttrs (rustPackageNames ++ rustExampleNames) (
        name:
        self.psycheLib.buildRustPackageWithPythonSidecar {
          inherit name;
          isExample = lib.elem name rustExampleNames;
        }
      )
    );

    nixglhostRustPackages = lib.listToAttrs (
      (map (
        name: lib.nameValuePair "${name}-nixglhost" (self.psycheLib.useHostGpuDrivers rustPackages.${name})
      ) rustPackageNames)
      ++ (map (
        name: lib.nameValuePair "${name}-nixglhost" (self.psycheLib.useHostGpuDrivers rustPackages.${name})
      ) rustExampleNames)
    );

    # Import Docker configurations
    dockerPackages = import ./docker.nix {
      inherit pkgs nixglhostRustPackages inputs;
    };

    psychePackages =
      {
        psyche-website-wasm = self.callPackage ../website/wasm { };
        psyche-website-shared = self.callPackage ../website/shared { };

        psyche-deserialize-zerocopy-wasm = self.psycheLib.buildRustWasmTsPackage "psyche-deserialize-zerocopy-wasm";

        solana-coordinator-idl = self.callPackage ../architectures/decentralized/solana-coordinator { };
        solana-mining-pool-idl = self.callPackage ../architectures/decentralized/solana-mining-pool { };

        psyche-book = self.callPackage ../psyche-book { inherit rustPackages rustPackageNames; };

      }
      // rustPackages
      // nixglhostRustPackages
      // dockerPackages;
  in
  {
    psycheLib = import ./lib.nix {
      inherit pkgs inputs;
    };

    inherit psychePackages;
  }
  // lib.mapAttrs (_: lib.id) psychePackages
)
