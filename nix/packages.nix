{ self, inputs, ... }:
{
  perSystem =
    { system, pkgs, ... }:
    let

      inherit (pkgs.psycheLib)
        cargoArtifacts
        rustToolchain
        craneLib
        buildSolanaIdl
        commonArgs
        buildRustPackage
        buildRustWasmTsPackage
        useHostGpuDrivers
        src
        ;

      solana-cli = inputs.solana-pkgs.packages.${system}.solana;

      rustPackageNames = [
        "psyche-solana-client"
        "psyche-centralized-client"
        "psyche-centralized-server"
        "psyche-centralized-local-testnet"
        "expand-distro"
        "preview-lr"
      ];

      rustPackages = builtins.listToAttrs (
        map (name: {
          inherit name;
          value = buildRustPackage name;
        }) rustPackageNames
      );

      nixglhostRustPackages = builtins.listToAttrs (
        map (name: {
          name = "${name}-nixglhost";
          value = useHostGpuDrivers rustPackages.${name};
        }) rustPackageNames
      );

      cudaDockerImage = pkgs.dockerTools.pullImage {
        imageName = "nvidia/cuda";
        imageDigest = "sha256:0f6bfcbf267e65123bcc2287e2153dedfc0f24772fb5ce84afe16ac4b2fada95";
        sha256 = "sha256-WFT1zPTFV9lAoNnT2jmKDB+rrbUSY9escmlO95pg4sA=";
      };
    in
    {
      packages =
        rustPackages
        // nixglhostRustPackages
        // rec {
          stream-docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
            name = "docker.io/nousresearch/psyche-centralized-client";
            tag = "latest";
            fromImage = cudaDockerImage;
            config = {
              contents = [ pkgs.bash ];
              Env = [
                "RUST_BACKTRACE=1"
                "TUI=false"
              ];
              Entrypoint = [
                "${rustPackages.psyche-centralized-client}/bin/psyche-centralized-client"
                "train"
                "--checkpoint-dir"
                "./checkpoints"
              ];
            };
          };

          # stream-docker-psyche-solana-client = pkgs.dockerTools.streamLayeredImage {
          #   name = "docker.io/nousresearch/psyche-solana-client";
          #   tag = "latest";

          #   contents = [
          #     pkgs.bashInteractive
          #     pkgs.coreutils
          #     solana-cli
          #     rustPackages.psyche-solana-client
          #   ];

          #   config = {
          #     Entrypoint = [
          #       "${pkgs.bashInteractive}/bin/bash"
          #       # "/bin/sh"
          #       # "-c"
          #       # "sleep 10 && /usr/local/run_owner_entrypoint.sh"
          #     ];
          #   };
          # };

          psyche-book = pkgs.callPackage ../psyche-book { inherit rustPackages rustPackageNames; };
        };
    };
}
