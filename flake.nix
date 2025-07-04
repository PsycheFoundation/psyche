{
  description = "Nous Psyche";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
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
    solana-pkgs.url = "github:arilotter/solana-flake";
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
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
      ];

      agenix-shell = {
        secrets = {
          devnet-keypair-wallet.file = ./secrets/devnet/wallet.age;
          devnet-rpc.file = ./secrets/devnet/rpc.age;
          mainnet-rpc.file = ./secrets/mainnet/rpc.age;
        };
      };

      perSystem =
        { system, ... }:
        {
          _module.args.pkgs = (
            import inputs.nixpkgs (
              {
                inherit system;
              }
              // (import ./nix/pkgs.nix {
                inherit inputs;
                gitcommit = self.rev or self.dirtyRev;
              })
            )
          );
        };
      imports = [
        inputs.agenix-shell.flakeModules.default
        inputs.treefmt-nix.flakeModule
        inputs.git-hooks-nix.flakeModule
        ./nix/formatter.nix
        ./nix/packages.nix
        ./nix/devShell.nix
        ./nix/checks.nix
        ./nix/nixosModules.nix
      ];
    };
}
