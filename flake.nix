{
  description = "Nous Psyche";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    agenix = {
      url = "github:ryantm/agenix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        home-manager.follows = "";
        darwin.follows = "";
      };
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    nix-gl-host = {
      url = "github:arilotter/nix-gl-host-rs";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        crane.follows = "crane";
        rust-overlay.follows = "rust-overlay";
        flake-parts.follows = "flake-parts";
      };
    };
    garnix-lib = {
      url = "github:garnix-io/garnix-lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    solana-pkgs = {
      url = "github:arilotter/solana-flake";
    };
    agenix-shell = {
      url = "github:aciceri/agenix-shell";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-parts.follows = "flake-parts";
        nix-github-actions.follows = "";
      };
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks-nix = {
      url = "github:cachix/git-hooks.nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-compat.follows = "";
      };
    };
    garnix-cli = {
      url = "github:arilotter/garnix-cli";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      agenix-shell = {
        secrets = {
          devnet-keypair-wallet.file = ./secrets/devnet/wallet.age;
          devnet-rpc.file = ./secrets/devnet/rpc.age;
          mainnet-rpc.file = ./secrets/mainnet/rpc.age;
        };
      };

      imports = [
        inputs.agenix-shell.flakeModules.default
        inputs.git-hooks-nix.flakeModule
        ./nix/formatter.nix
        ./nix/packages.nix
        ./nix/devShell.nix
        ./nix/checks.nix
        ./nix/nixosModules.nix
      ];

      perSystem =
        {
          pkgs,
          lib,
          ...
        }:
        let
          # Setup uv2nix workspace per system
          workspace = inputs.uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./python; };

          overlay = workspace.mkPyprojectOverlay {
            sourcePreference = "wheel";
          };

          pythonSet =
            (pkgs.callPackage inputs.pyproject-nix.build.packages {
              python = pkgs.python312;
            }).overrideScope
              (
                lib.composeManyExtensions [
                  inputs.pyproject-build-systems.overlays.wheel
                  overlay
                  (
                    let
                      prebuilt =
                        packageNames: final: _prev:
                        let
                          inherit (final) pkgs;
                          hacks = pkgs.callPackage inputs.pyproject-nix.build.hacks { };

                          makePrebuilt = name: {
                            inherit name;
                            value = hacks.nixpkgsPrebuilt {
                              from = pkgs.python312Packages.${name};
                            };
                          };
                        in
                        builtins.listToAttrs (map makePrebuilt packageNames);
                    in
                    prebuilt [
                      "torch"
                      "liger-kernel"
                      "flash-attn"
                    ]
                  )
                ]
              );
        in
        {
          _module.args = {
            inherit pythonSet;
          };

          legacyPackages = {
            inherit pythonSet;
          };
        };
    };
}
