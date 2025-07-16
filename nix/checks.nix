{ lib, ... }:
{
  perSystem =
    {
      system,
      pkgs,
      self',
      ...
    }:
    let
      inherit (pkgs.psycheLib)
        craneLib
        rustWorkspaceArgs
        rustWorkspaceArgsWithPython
        cargoArtifacts
        ;
    in
    {
      checks = {
        workspace-clippy = craneLib.cargoClippy (
          rustWorkspaceArgs
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--workspace -- --deny warnings";
          }
        );

        workspace-test = craneLib.cargoNextest (
          rustWorkspaceArgsWithPython
          // {
            inherit cargoArtifacts;
            RUST_LOG = "info,psyche=trace";
            partitions = 1;
            partitionType = "count";
            cargoNextestExtraArgs = "--workspace --exclude psyche-decentralized-testing";
          }
        );

        validate-all-configs =
          pkgs.runCommandNoCC "validate-configs"
            { nativeBuildInputs = [ self'.packages.psyche-centralized-server ]; }
            ''
              dir="${../config}"
              if [ ! -d "$dir" ]; then
                echo "config dir $dir does not exist."
                exit 1
              fi

              for f in $dir/*; do
                if [ -f $f/data.toml ]; then
                  psyche-centralized-server validate-config --state $f/state.toml --data-config $f/data.toml || exit 1
                  echo "config $f/data.toml and $f/state.toml ok!"
                else
                  psyche-centralized-server validate-config --state $f/state.toml|| exit 1
                  echo "config $f/state.toml ok!"
                fi
              done;

              echo "all configs ok!"

              touch $out
            '';
      };
    };
}
