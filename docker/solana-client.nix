{
  psychePackages,
  dockerTools,
  runCommand,
  bashInteractive,
  cacert,
  coreutils,
}:
dockerTools.streamLayeredImage {
  name = "psyche-solana-client";
  tag = "latest";

  contents = [
    bashInteractive
    cacert
    coreutils
    psychePackages.psyche-solana-client-nixglhost
    psychePackages.psyche-centralized-client-nixglhost
    psychePackages.inference-nixglhost
    psychePackages.train-nixglhost
    psychePackages.bandwidth_test-nixglhost
    (runCommand "entrypoint" { } ''
      mkdir -p $out/bin $out/etc $out/tmp $out/var/tmp $out/run
      cp ${./train_entrypoint.sh} $out/bin/train_entrypoint.sh
      chmod +x $out/bin/train_entrypoint.sh
    '')
  ];

  config = {
    Env = [
      "NVIDIA_DRIVER_CAPABILITIES=all"
    ];
    Entrypoint = [ "/bin/train_entrypoint.sh" ];
  };
}
