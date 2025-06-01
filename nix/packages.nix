{ self, ... }:
{
  perSystem =
    {
      system,
      pkgs,
      inputs',
      ...
    }:
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

      solana-cli = inputs'.solana-pkgs.packages.solana;

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
          psyche-book = pkgs.callPackage ../psyche-book { inherit rustPackages rustPackageNames; };
          psyche-solana-client-docker = pkgs.dockerTools.buildImage {
                  name = "nousresearch/psyche-solana-client";
                  tag = "latest";
                  fromImage = cudaDockerImage;

                  # Copy the binary and the entrypoint script into the image
                  copyToRoot = pkgs.buildEnv {
                    name = "root";
                    paths = [
                      pkgs.bashInteractive
                      rustPackages."psyche-solana-client"
                      (pkgs.runCommand "entrypoint" {} ''
                        mkdir -p $out/bin
                        cp ${../docker/train_entrypoint.sh} $out/bin/train_entrypoint.sh
                        chmod +x $out/bin/train_entrypoint.sh
                      '')
                    ];
                    pathsToLink = [ "/bin" ];
                  };

                  config = {
                    Env = [
                      "RUST_BACKTRACE=1"
                      "TUI=false"
                    ];
                    Entrypoint = [ "/bin/train_entrypoint.sh" ];
                  };
                };
        };
    };
}
