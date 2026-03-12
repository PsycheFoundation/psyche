{
  pkgs,
  lib ? pkgs.lib,
  rustPackages,
  ...
}:
let
  layeringPipeline = pkgs.writeText "reverse-popularity-layering.json" ''
    [
      ["popularity_contest"],
      ["reverse"],
      ["limit_layers", 99]
    ]
  '';

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
