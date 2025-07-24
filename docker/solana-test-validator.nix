{
  psycheLib,
  psychePackages,
  dockerTools,
  bashInteractive,
  bzip2,
  gnutar,
  gnugrep,
  coreutils,
  runCommand,
}:
let
  solana = psycheLib.solanaPkgs.default;
  coordinatorSo = psychePackages.solana-coordinator.so;
  authorizerSo = psychePackages.solana-authorizer.so;
in

dockerTools.streamLayeredImage {
  name = "psyche-solana-test-validator";
  tag = "latest";

  contents = [
    bashInteractive
    bzip2
    gnutar
    solana
    gnugrep
    coreutils

    (runCommand "copy-solana-programs" { } ''
      mkdir -p $out/bin
      mkdir -p $out/local
      chmod 755 $out/local
      cp ${./test/psyche_solana_validator_entrypoint.sh} $out/bin/psyche_solana_validator_entrypoint.sh
      ls -r ${coordinatorSo}
      ls -r ${authorizerSo}
      exit 1
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
}
