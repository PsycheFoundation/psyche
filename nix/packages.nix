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
      anchor-cli = inputs'.solana-pkgs.packages.anchor;

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
                (pkgs.runCommand "entrypoint" { } ''
                  mkdir -p $out/bin
                  cp ${../docker/train_entrypoint.sh} $out/bin/train_entrypoint.sh
                  chmod +x $out/bin/train_entrypoint.sh
                '')
              ];
              pathsToLink = [ "/bin" ];
            };

            config = {
              Entrypoint = [ "/bin/train_entrypoint.sh" ];
            };
          };

          psyche-solana-test-client-docker = pkgs.dockerTools.buildImage {
            name = "psyche-test-client";
            tag = "latest";
            fromImage = cudaDockerImage;

            # Copy the binary and the entrypoint script into the image
            copyToRoot = pkgs.buildEnv {
              name = "root";
              paths = [
                pkgs.solana-cli
                pkgs.bashInteractive
                rustPackages."psyche-solana-client"
                (pkgs.runCommand "entrypoint" { } ''
                    mkdir -p $out/bin
                    cp ${../docker/test/client_test_entrypoint.sh} $out/bin/client_test_entrypoint.sh
                    cp ${../docker/test/run_owner_entrypoint.sh} $out/bin/run_owner_entrypoint.sh
                    chmod +x $out/bin/client_test_entrypoint.sh
                    chmod +x $out/bin/run_owner_entrypoint.sh
                '')
              ];
              pathsToLink = [ "/bin" ];
            };

            config = {
              Entrypoint = [ "/bin/client_test_entrypoint.sh" ];
            };
          };

          psyche-solana-test-validator = pkgs.dockerTools.buildImage {
            name = "psyche-solana-test-validator";
            tag = "latest";

            # Copy the binary and the entrypoint script into the image
            copyToRoot = pkgs.buildEnv {
              name = "root";
              paths = [
                pkgs.coreutils
                solana-cli
                anchor-cli
                pkgs.bashInteractive
                (pkgs.runCommand "entrypoint" { } ''
                    mkdir -p $out/bin
                    mkdir -p $out/lib
                    cp ${../docker/test/psyche_solana_validator_entrypoint.sh} $out/bin/psyche_solana_validator_entrypoint.sh
                    cp -r ${../architectures/decentralized/solana-coordinator} $out/lib/solana-coordinator
                    cp -r ${../architectures/decentralized/solana-authorizer} $out/lib/solana-authorizer
                    chmod +x $out/bin/psyche_solana_validator_entrypoint.sh
                '')
              ];
              pathsToLink = [ "/bin" "/lib" ];
            };

            config = {
              Entrypoint = [ "/bin/psyche_solana_validator_entrypoint.sh" ];
              ExposedPorts = {
                  "8899/tcp" =  { };
                  "8900/tcp" = { };
              };
            };
          };
        };
    };
}
