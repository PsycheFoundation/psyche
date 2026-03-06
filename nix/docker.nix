{
  pkgs,
  lib ? pkgs.lib,
  rustPackages,
  inputs,
  externalRustPackages,
  solanaCoordinatorProgram,
  solanaAuthorizerProgram,
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  solana = inputs.solana-pkgs.packages.${system}.default;
  anchor = inputs.solana-pkgs.packages.${system}.anchor;

  layeringPipeline = pkgs.writeText "reverse-popularity-layering.json" ''
    [
      ["popularity_contest"],
      ["reverse"],
      ["limit_layers", 99]
    ]
  '';

  mkSolanaTestClientImage =
    {
      imageName,
      solanaClientPackage,
      usePython ? false,
    }:
    pkgs.dockerTools.streamLayeredImage {
      name = imageName;
      tag = "latest";
      contents =
        with pkgs;
        [
          solana
          bashInteractive
          busybox
          cacert
          solanaClientPackage
          externalRustPackages.solana-toolbox-cli
          jq
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
            mkdir -p $out/architectures/decentralized/solana-authorizer/target/deploy
            cp ${../docker/test/client_test_entrypoint.sh} $out/bin/client_test_entrypoint.sh
            cp ${../docker/test/run_owner_entrypoint.sh} $out/bin/run_owner_entrypoint.sh
            chmod +x $out/bin/client_test_entrypoint.sh
            chmod +x $out/bin/run_owner_entrypoint.sh
          '')
        ]
        ++ lib.optionals usePython [
          coreutils
          stdenv.cc
          rdma-core
        ];

      config = {
        Env = [
          "NVIDIA_DRIVER_CAPABILITIES=compute,utility"
          "NVIDIA_VISIBLE_DEVICES=all"
          "LOGNAME=root"
          "TORCHINDUCTOR_CACHE_DIR=/tmp/torchinductor"
          "PYTHONUNBUFFERED=1"
          "PYTHON_ENABLED=${if usePython then "true" else "false"}"
        ];
        Entrypoint = [ "/bin/client_test_entrypoint.sh" ];
      };
    };

  dockerPackages = {
    docker-psyche-solana-client = pkgs.dockerTools.streamLayeredImage {
      name = "psyche-solana-client";
      tag = "latest";

      contents = with pkgs; [
        bashInteractive
        cacert
        coreutils
        stdenv.cc
        rdma-core
        rustPackages."psyche-solana-client"
        rustPackages."psyche-centralized-client"
        rustPackages."inference"
        rustPackages."train"
        rustPackages."bandwidth_test"
        rustPackages."psyche-sidecar"
        python3Packages.huggingface-hub
        (pkgs.runCommand "entrypoint" { } ''
          mkdir -p $out/bin $out/etc $out/tmp $out/var/tmp $out/run
          cp ${../docker/train_entrypoint.sh} $out/bin/train_entrypoint.sh
          cp ${../docker/sidecar_entrypoint.sh} $out/bin/sidecar_entrypoint.sh
          chmod +x $out/bin/train_entrypoint.sh
          chmod +x $out/bin/sidecar_entrypoint.sh
        '')
      ];

      config = {
        Env = [
          "NVIDIA_DRIVER_CAPABILITIES=all"
          "LD_LIBRARY_PATH=/lib:/usr/lib"
          "LOGNAME=root"
          "TORCHINDUCTOR_CACHE_DIR=/tmp/torchinductor"
          "PYTHONUNBUFFERED=1"
        ];
        Entrypoint = [ "/bin/train_entrypoint.sh" ];
      };

      inherit layeringPipeline;
    };

    docker-psyche-solana-test-client = mkSolanaTestClientImage {
      imageName = "psyche-solana-test-client";
      solanaClientPackage = rustPackages."psyche-solana-client";
      usePython = true;
    };

    docker-psyche-solana-test-client-no-python = mkSolanaTestClientImage {
      imageName = "psyche-solana-test-client";
      solanaClientPackage = rustPackages."psyche-solana-client-nopython";
      usePython = false;
    };

    docker-psyche-solana-test-validator = pkgs.dockerTools.streamLayeredImage {
      name = "psyche-solana-test-validator";
      tag = "latest";

      contents = with pkgs; [
        bashInteractive
        bzip2
        gnutar
        solana
        anchor
        gnugrep
        coreutils

        (pkgs.runCommand "copy-solana-programs" { } ''
          mkdir -p $out/bin

          # Coordinator
          mkdir -p $out/local/solana-coordinator/target/deploy
          cp ${solanaCoordinatorProgram}/*.so $out/local/solana-coordinator/target/deploy/
          cp ${solanaCoordinatorProgram}/*-keypair.json $out/local/solana-coordinator/target/deploy/

          # Authorizer
          mkdir -p $out/local/solana-authorizer/target/deploy
          mkdir -p $out/local/solana-authorizer/target/idl
          cp ${solanaAuthorizerProgram}/*.so $out/local/solana-authorizer/target/deploy/
          cp ${solanaAuthorizerProgram}/*-keypair.json $out/local/solana-authorizer/target/deploy/
          # Copy IDL (exclude keypair json)
          for f in ${solanaAuthorizerProgram}/*.json; do
            case "$f" in *-keypair.json) ;; *) cp "$f" $out/local/solana-authorizer/target/idl/ ;; esac
          done

          # Entrypoint
          cp ${../docker/test/psyche_solana_validator_entrypoint.sh} $out/bin/psyche_solana_validator_entrypoint.sh
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

    docker-psyche-gateway-node = pkgs.dockerTools.streamLayeredImage {
      name = "psyche-gateway-node";
      tag = "latest";

      contents = [
        pkgs.cacert
        rustPackages."bin-psyche-inference-node-gateway-node"
        (pkgs.runCommand "gateway-setup" { } ''
          mkdir -p $out/tmp
        '')
      ];

      config = {
        Entrypoint = [ "/bin/bin-psyche-inference-node-gateway-node" ];
        ExposedPorts = {
          "8000/tcp" = { };
        };
      };
    };

    docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
      name = "psyche-centralized-client";
      tag = "latest";

      # Copy the binary and the entrypoint script into the image

      contents = [
        pkgs.bashInteractive
        rustPackages."psyche-centralized-client"
      ];

      config = {
        Env = [
          "NVIDIA_DRIVER_CAPABILITIES=compute,utility"
          "NVIDIA_VISIBLE_DEVICES=all"
          "LOGNAME=root"
          "TORCHINDUCTOR_CACHE_DIR=/tmp/torchinductor"
          "TRITON_=/usr/lib64"
          "PYTHONUNBUFFERED=1"
        ];
      };
    };
  };
in
lib.optionalAttrs pkgs.stdenv.isLinux dockerPackages
