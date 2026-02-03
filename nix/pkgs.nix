{
  pkgs,
  lib ? pkgs.lib,
  inputs,
}:
lib.makeScope pkgs.newScope (
  self:
  let
    psycheLib = import ./lib.nix {
      inherit pkgs inputs;
    };
    util = import ./util.nix;

    inherit (psycheLib) rustPackages;

    externalRustPackages = {
      solana-toolbox-cli = pkgs.rustPlatform.buildRustPackage rec {
        pname = "solana_toolbox_cli";
        version = "0.4.3";

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

    dockerPackages = import ./docker.nix {
      inherit
        pkgs
        inputs
        rustPackages
        externalRustPackages
        ;
    };

    psychePackages = (
      util.mergeAttrsetsNoConflicts "can't merge psyche package sets." [
        {
          psyche-website-shared = self.callPackage ../website/shared { };

          # WASM packages use special build process
          psyche-deserialize-zerocopy-wasm = psycheLib.buildRustWasmTsPackage "psyche-deserialize-zerocopy-wasm";
          psyche-website-wasm = self.callPackage ../website/wasm { };

          psyche-website-backend = self.callPackage ../website/backend { };

          solana-coordinator-idl = self.callPackage ../architectures/decentralized/solana-coordinator { };
          solana-mining-pool-idl = self.callPackage ../architectures/decentralized/solana-mining-pool { };

          psyche-book = self.callPackage ../psyche-book { inherit rustPackages; };
        }
        rustPackages
        externalRustPackages
        dockerPackages
      ]
    );
  in
  {
    inherit psycheLib psychePackages;
  }
  // lib.mapAttrs (_: lib.id) psychePackages
)
