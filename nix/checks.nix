{ ... }:
{
  perSystem =
    {
      pkgs,
      self',
      ...
    }:
    let
      inherit (pkgs.psycheLib)
        craneLib
        rustWorkspaceArgs
        mkRustWorkspaceArgsWithPython
        mkPythonVenv
        psychePythonExtension
        allPythonDeps
        cargoArtifacts
        ;

      rustWorkspaceArgsWithPython = mkRustWorkspaceArgsWithPython (mkPythonVenv {
        extraPackages = {
          psyche = psychePythonExtension;
        };
        additionalNixPackages = allPythonDeps;
      });
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

          workspace-test-all = testWithProfile "default";

          workspace-test-ci = testWithProfile "ci";

          workspace-test-decentralized = testWithProfile "decentralized";

          workspace-test-parallelism = testWithProfile "parallelism";

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
