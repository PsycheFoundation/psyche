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
      checks =
        let
          testWithProfile =
            profile:
            craneLib.cargoNextest (
              rustWorkspaceArgsWithPython
              // {
                inherit cargoArtifacts;

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

          workspace-test-two-clients-three-epochs = craneLib.cargoNextest (
            rustWorkspaceArgsWithPython
            // {
              inherit cargoArtifacts;

              RUST_LOG = "info,psyche=trace";
              partitions = 1;
              partitionType = "count";
              cargoNextestExtraArgs = "--workspace --profile two-clients-three-epochs";

              nativeBuildInputs =
                rustWorkspaceArgsWithPython.nativeBuildInputs
                ++ (with pkgs; [
                  docker
                  procps
                  coreutils
                ]);

              # Allow network access and disable chroot for Docker
              __noChroot = true;

              preBuild = ''
                # Setup rootless Docker
                export XDG_RUNTIME_DIR=$TMPDIR/xdg
                mkdir -p $XDG_RUNTIME_DIR
                export DOCKER_HOST=unix://$XDG_RUNTIME_DIR/docker.sock
                # Start rootless Docker daemon in background
                dockerd-rootless.sh --data-root $TMPDIR/docker-data --exec-root $TMPDIR/docker-exec &
                DOCKER_PID=$!
                # Wait for Docker to be ready
                timeout=30
                while [ $timeout -gt 0 ] && ! docker version >/dev/null 2>&1; do
                  sleep 1
                  timeout=$((timeout-1))
                done
                if [ $timeout -eq 0 ]; then
                  echo "Docker failed to start"
                  kill $DOCKER_PID || true
                  exit 1
                fi
                echo "Docker is ready"
              '';

              postBuild = ''
                # Cleanup Docker
                kill $DOCKER_PID || true
                wait $DOCKER_PID || true
              '';
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
