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

    workspaceCargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);

    # expand globs in workspace members from cargo.toml
    expandWorkspaceMembers =
      members:
      lib.flatten (
        lib.map (
          memberPattern:
          if lib.hasSuffix "/*" memberPattern then
            let
              dir = lib.removeSuffix "/*" memberPattern;
              dirPath = ../${dir};
              entries = builtins.readDir dirPath;
              subdirs = lib.filterAttrs (n: v: v == "directory") entries;
            in
            lib.mapAttrsToList (name: _: "${dir}/${name}") subdirs
          else
            [ memberPattern ]
        ) members
      );

    expandedMembers = expandWorkspaceMembers workspaceCargoToml.workspace.members;

    # find all crates with packages.nix
    discoverCratesWithPackagesNix =
      members:
      lib.filter (pkg: pkg != null) (
        lib.map (
          memberPath:
          let
            fullPath = ../${memberPath};
            packagesNixPath = fullPath + "/packages.nix";
            cargoTomlPath = fullPath + "/Cargo.toml";

            isExcluded = builtins.elem memberPath [
              "python/" # python venv with special dependencies
            ];

            hasCargoToml = builtins.pathExists cargoTomlPath;
            hasPackagesNix = builtins.pathExists packagesNixPath;
          in
          if hasCargoToml && hasPackagesNix && !isExcluded then
            let
              cargoToml = builtins.fromTOML (builtins.readFile cargoTomlPath);
              packageName = cargoToml.package.name or (baseNameOf memberPath);
            in
            {
              name = packageName;
              path = fullPath;
            }
          else
            null
        ) members
      );

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

    # a packages.nix returns an attrset of packages (including examples)
    rustPackages = util.mergeAttrsetsNoConflicts "can't merge rust package sets." (
      lib.map (pkg: import (pkg.path + "/packages.nix") { inherit psycheLib pkgs inputs; }) (
        discoverCratesWithPackagesNix expandedMembers
      )
    );

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
          solana-distributor-idl = self.callPackage ../architectures/decentralized/solana-distributor { };

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
