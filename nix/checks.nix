{ ... }:
{
  perSystem =
    {
      pkgs,
      self',
      inputs',
      ...
    }:
    let
      inherit (pkgs.psycheLib)
        craneLib
        rustWorkspaceArgs
        rustWorkspaceArgsWithPython
        cargoArtifacts
        ;

      solanaTests = [
        "test_one_clients_three_epochs_run"
        "test_two_clients_three_epochs_run"
        "test_client_join_and_get_model_p2p"
        # test_rejoining_client_delay requires network delay (CAP_NET_ADMIN) — #[ignore]d
        "disconnect_client"
        "drop_a_client_waitingformembers_then_reconnect"
        "test_when_all_clients_disconnect_checkpoint_is_hub"
        "test_everybody_leaves_in_warmup"
        "test_lost_only_peer_go_back_to_hub_checkpoint"
        "test_pause_and_resume_run"
        # test_solana_rpc_fallback requires nginx proxies — not yet ported to subprocess mode
      ];

      mkTestCheck =
        testName:
        pkgs.runCommand "solana-test-${testName}"
          {
            nativeBuildInputs = [
              inputs'.solana-pkgs.packages.solana
              self'.packages.run-manager
              self'.packages.psyche-solana-client
              pkgs.coreutils
              pkgs.gnugrep
              pkgs.gnutar
              pkgs.bzip2
            ];

            SOLANA_PROGRAMS_DIR = self'.packages.solana-coordinator-program;
            SOLANA_AUTHORIZER_DIR = self'.packages.solana-authorizer-program;
            TEST_BASE_CONFIG_PATH = ../config/solana-test/nano-config.toml;
            HOME = "./tmp-home";
          }
          ''

            mkdir -p "$HOME"
            echo "running test: ${testName}"
            ${
              pkgs.lib.getExe self'.packages."test-psyche-decentralized-testing-integration_tests"
            } --nocapture "${testName}"
            touch $out
          '';

      integrationTests = builtins.listToAttrs (
        map (testName: {
          name = "solana-integration-test-${testName}";
          value = mkTestCheck testName;
        }) solanaTests
      );
    in
    {
      checks =
        let
          testWithProfile =
            profile:
            craneLib.cargoNextest (
              rustWorkspaceArgsWithPython
              // {
                inherit cargoArtifacts;
                RUST_BACKTRACE = "full";
                RUST_LOG = "info,psyche=trace";
                partitions = 1;
                partitionType = "count";
                cargoNextestExtraArgs = "--workspace --profile ${profile}";
              }
            );
        in
        {
          workspace-clippy = craneLib.cargoClippy (
            rustWorkspaceArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--workspace -- --deny warnings";
            }
          );

          workspace-test-ci = testWithProfile "ci";

          validate-all-configs =
            pkgs.runCommand "validate-configs"
              { nativeBuildInputs = [ self'.packages.psyche-centralized-server ]; }
              ''
                export NIXGL_HOST_CACHE_DIR=$TMPDIR/nixglhost
                dir="${../config}"
                if [ ! -d "$dir" ]; then
                  echo "config dir $dir does not exist."
                  exit 1
                fi


                for f in $dir/*; do
                  if [ -f $f/data.toml ]; then
                  echo "ccecking $(realpath -s --relative-to $dir $f/data.toml) and $(realpath -s --relative-to $dir $f/state.toml)"
                    psyche-centralized-server validate-config --state $f/state.toml --data-config $f/data.toml || exit 1
                    echo "ok!"
                  elif [ -f $f/state.toml ]; then
                    echo "checking $(realpath -s --relative-to $dir $f/state.toml)"
                    psyche-centralized-server validate-config --state $f/state.toml || exit 1
                    echo "ok!"
                  else
                    echo "Note: $(realpath -s --relative-to $dir $f) has no state.toml, skipping validation"
                  fi
                done;


                echo "all configs ok!"

                touch $out
              '';
        }
        // integrationTests;
    };
}
