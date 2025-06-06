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

      # We need this because the solana validator require the compiled .so files of the Solana programs,
      # but since nix can't copy those files using a relative path because they're not tracked by git,
      # we have to use an absolute path and mark it impure to make this work as expected.
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
          docker-psyche-solana-client = pkgs.dockerTools.streamLayeredImage {
            name = "nousresearch/psyche-solana-client";
            tag = "latest";

            # Copy the binary and the entrypoint script into the image
            contents = with pkgs; [
              bashInteractive
              wget
              curl
              cacert
              coreutils
              nixglhostRustPackages."psyche-solana-client-nixglhost"
              (pkgs.runCommand "entrypoint" { } ''
                mkdir -p $out/bin $out/etc $out/tmp $out/var/tmp $out/run
                cp ${../docker/train_entrypoint.sh} $out/bin/train_entrypoint.sh
                chmod +x $out/bin/train_entrypoint.sh
              '')
            ];

            config = {
              Env = [
                "NVIDIA_DRIVER_CAPABILITIES=all"
                "LD_LIBRARY_PATH=${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.cudatoolkit}/lib:${pkgs.cudatoolkit.lib}/lib:${pkgs.cudaPackages.cudnn}/lib:/usr/lib64"
              ];
              Entrypoint = [ "/bin/train_entrypoint.sh" ];
            };
          };

          docker-psyche-solana-test-client = pkgs.dockerTools.streamLayeredImage {
            name = "psyche-test-client";
            tag = "latest";

            contents = with pkgs; [
              inputs'.solana-pkgs.packages.solana
              bashInteractive
              busybox
              cacert
              wget
              curl
              nixglhostRustPackages."psyche-solana-client-nixglhost"

              # Create proper system structure including /tmp
              (pkgs.runCommand "system-setup" { } ''
                mkdir -p $out/etc $out/tmp $out/var/tmp $out/run

                # Create basic passwd and group files
                cat > $out/etc/passwd << EOF
                  root:x:0:0:root:/root:/bin/bash
                  nobody:x:65534:65534:nobody:/nonexistent:/bin/false
                  EOF

                cat > $out/etc/group << EOF
                  root:x:0:
                  nobody:x:65534:
                  EOF

                # Set proper permissions for temp directories
                chmod 1777 $out/tmp
                chmod 1777 $out/var/tmp
                chmod 755 $out/run
              '')

              (pkgs.runCommand "entrypoint" { } ''
                mkdir -p $out/bin
                cp ${../docker/test/client_test_entrypoint.sh} $out/bin/client_test_entrypoint.sh
                cp ${../docker/test/run_owner_entrypoint.sh} $out/bin/run_owner_entrypoint.sh
                chmod +x $out/bin/client_test_entrypoint.sh
                chmod +x $out/bin/run_owner_entrypoint.sh
              '')
            ];

            config = {
              Env = [
                "LD_LIBRARY_PATH=${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.cudatoolkit}/lib:${pkgs.cudatoolkit.lib}/lib:${pkgs.cudaPackages.cudnn}/lib:/usr/lib64"
                "NVIDIA_DRIVER_CAPABILITIES=compute,utility"
                "NVIDIA_VISIBLE_DEVICES=all"
              ];
              Entrypoint = [ "/bin/client_test_entrypoint.sh" ]; # Use debug entrypoint first
            };
          };

          docker-psyche-solana-test-validator = pkgs.dockerTools.streamLayeredImage {
            name = "psyche-solana-test-validator";
            tag = "latest";

            # Use buildImage instead of streamLayeredImage for better compatibility
            contents = with pkgs; [
              bashInteractive
              bzip2
              gnutar
              inputs'.solana-pkgs.packages.default
              gnugrep
              coreutils

              (pkgs.runCommand "copy-solana-programs" { } ''
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

            config = {
              WorkingDir = "/tmp";
              Entrypoint = [ "/bin/psyche_solana_validator_entrypoint.sh" ];
              ExposedPorts = {
                "8899/tcp" = { };
                "8900/tcp" = { };
              };
            };
          };

          docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
            name = "psyche-centralized-client";
            tag = "latest";
            fromImage = cudaDockerImage;

            # Copy the binary and the entrypoint script into the image

            contents = [
              pkgs.bashInteractive
              nixglhostRustPackages."psyche-centralized-client-nixglhost"
            ];

            config = {
              Env = [ "NVIDIA_DRIVER_CAPABILITIES=all" ];
            };
          };
        };
    };
}
