{ lib, inputs, ... }:
{
  perSystem =
    {
      system,
      pkgs,
      inputs',
      self',
      ...
    }:
    let
      inherit (pkgs.psycheLib) rustWorkspaceArgs;

      # all the deps needed to run integration tests
      testDeps =
        with pkgs;
        [
          just
          docker
          coreutils
          bash
        ]
        ++ (with inputs'.solana-pkgs.packages; [
          solana
          anchor
        ])
        ++ rustWorkspaceArgs.buildInputs
        ++ rustWorkspaceArgs.nativeBuildInputs;

      makeIntegrationTestApp =
        {
          name,
          testName,
        }:
        {
          type = "app";
          program =
            let
              script = pkgs.writeShellScriptBin name ''
                set -euo pipefail

                echo "setting up rootless docker..."
                export XDG_RUNTIME_DIR=/tmp/docker-runtime-$$
                mkdir -p $XDG_RUNTIME_DIR
                export DOCKER_HOST=unix://$XDG_RUNTIME_DIR/docker.sock

                # start dockerd in rootless mode
                ${pkgs.docker}/bin/dockerd-rootless.sh &
                DOCKER_PID=$!

                cleanup() {
                  echo "cleaning up docker..."
                  kill $DOCKER_PID 2>/dev/null || true
                  rm -rf $XDG_RUNTIME_DIR
                }
                trap cleanup EXIT

                # wait for docker to be ready
                echo "waiting for docker to start..."
                timeout 60 sh -c "until ${pkgs.docker}/bin/docker info >/dev/null 2>&1; do sleep 1; done"

                echo "docker is ready, running integration test..."
                cd ${inputs.self}
                export PATH="${lib.makeBinPath testDeps}:$PATH"
                ${pkgs.just}/bin/just decentralized-integration-test ${testName}
              '';
            in
            "${script}/bin/${name}";
        };
    in
    {
      apps = {
        decentralized-integration-test = makeIntegrationTestApp {
          name = "run-decentralized-integration-test";
          testName = "test_two_clients_three_epochs_run";
        };
      };
    };
}
