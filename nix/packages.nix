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
        finalImageTag = "12.4.1-devel-ubuntu22.04";
        imageDigest = "sha256:da6791294b0b04d7e65d87b7451d6f2390b4d36225ab0701ee7dfec5769829f5";
        sha256 = "sha256-T4HwY8M0XMqh0rpK5zz2+n5/6FhBzLj7gtgbtJARJKg=";
      };

      debianDockerImage = pkgs.dockerTools.pullImage {
        imageName = "debian";
        imageDigest = "sha256:90522eeb7e5923ee2b871c639059537b30521272f10ca86fdbbbb2b75a8c40cd"; # optional but recommended for reproducibility
        finalImageTag = "bookworm-slim";
        sha256 = "sha256-8w3qrMGmG/id87EzoE5h4gk+MNStygF+eS1j6/kSUe8=";
      };

      psycheHome = builtins.getEnv "PSYCHE_HOME";
      coordinatorSrc = builtins.path {
        path = "${psycheHome}/architectures/decentralized/solana-coordinator";
        name = "solana-coordinator";
      };
      authorizerSrc = builtins.path {
        path = "${psycheHome}/architectures/decentralized/solana-authorizer";
        name = "solana-authorizer";
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
                nixglhostRustPackages."psyche-solana-client-nixglhost"
                (pkgs.runCommand "entrypoint" { } ''
                  mkdir -p $out/bin
                  cp ${../docker/train_entrypoint.sh} $out/bin/train_entrypoint.sh
                  chmod +x $out/bin/train_entrypoint.sh
                '')
              ];
              pathsToLink = [ "/bin" ];
            };

            config = {
              Env = [ "NVIDIA_DRIVER_CAPABILITIES=all" ];
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
                inputs'.solana-pkgs.packages.solana
                pkgs.bashInteractive
                nixglhostRustPackages."psyche-solana-client-nixglhost"
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
              Env = [ "NVIDIA_DRIVER_CAPABILITIES=all" ];
              Entrypoint = [ "/bin/client_test_entrypoint.sh" ];
            };
          };

          psyche-solana-test-validator = pkgs.dockerTools.buildImage {
            name = "psyche-solana-test-validator";
            tag = "latest";
            fromImage = debianDockerImage;

            # Copy the binary and the entrypoint script into the image
            copyToRoot = pkgs.buildEnv {
              name = "root";
              paths = [
                pkgs.bashInteractive
                pkgs.bzip2
                inputs'.solana-pkgs.packages.default
                (pkgs.runCommand "entrypoint" { } ''
                  mkdir -p $out/bin
                  mkdir -p $out/local
                  chmod 755 $out/local
                  cp ${../docker/test/psyche_solana_validator_entrypoint.sh} $out/bin/psyche_solana_validator_entrypoint.sh
                  cp -r ${coordinatorSrc} $out/local
                  cp -r ${authorizerSrc} $out/local
                  mv $out/local/*solana-coordinator $out/local/solana-coordinator
                  mv $out/local/*solana-authorizer $out/local/solana-authorizer
                  chmod +x $out/bin/psyche_solana_validator_entrypoint.sh
                '')
              ];
              pathsToLink = [
                "/bin"
                "/local"
              ];
            };

            config = {
              Entrypoint = [ "/bin/psyche_solana_validator_entrypoint.sh" ];
              ExposedPorts = {
                "8899/tcp" = { };
                "8900/tcp" = { };
              };
            };
          };
        };
    };
}
