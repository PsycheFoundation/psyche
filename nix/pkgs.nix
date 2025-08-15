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
      "psyche-sidecar"
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

    externalRustPackages = {
      solana_toolbox_cli = pkgs.rustPlatform.buildRustPackage rec {
        pname = "solana_toolbox_cli";
        version = "0.4.3"; # Replace with actual version

        src = pkgs.fetchCrate {
          inherit pname version;
          sha256 = "sha256-6bCbFtVAs4MctSYslTNBk859LxfdOjwetvq/1Ub3VVg=";
        };

        cargoHash = "sha256-cQ8XkfWdU2HxYnyZQNC59lWWDMbJ0OLocmTiH+N5zrc=";

        nativeBuildInputs = with pkgs; [
          pkg-config
          perl
        ];
        buildInputs = with pkgs; [ openssl ];
      };
    };

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
      inherit
        pkgs
        nixglhostRustPackages
        inputs
        externalRustPackages
        ;
    };

    psychePackages = {
      psyche-website-wasm = self.callPackage ../website/wasm { };
      psyche-website-shared = self.callPackage ../website/shared { };

      psyche-deserialize-zerocopy-wasm = self.psycheLib.buildRustWasmTsPackage "psyche-deserialize-zerocopy-wasm";

      solana-coordinator-idl = self.callPackage ../architectures/decentralized/solana-coordinator { };
      solana-mining-pool-idl = self.callPackage ../architectures/decentralized/solana-mining-pool { };

      psyche-book = self.callPackage ../psyche-book { inherit rustPackages rustPackageNames; };
    }
    // rustPackages
    // externalRustPackages
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
