# Garnix action apps for integration tests and Docker image pushing.
{ inputs, ... }:
{
  perSystem =
    {
      pkgs,
      self',
      lib,
      system,
      ...
    }:
    let
      solanaTests = [
        "test_one_clients_three_epochs_run"
        "test_two_clients_three_epochs_run"
        "test_client_join_and_get_model_p2p"
        "test_rejoining_client_delay"
        "disconnect_client"
        "drop_a_client_waitingformembers_then_reconnect"
        "test_when_all_clients_disconnect_checkpoint_is_hub"
        "test_everybody_leaves_in_warmup"
        "test_lost_only_peer_go_back_to_hub_checkpoint"
        "test_pause_and_resume_run"
        "test_solana_rpc_fallback"
      ];

      # Common podman setup for garnix actions:
      # - creates containers policy.json
      # - starts podman API service (for docker-compose / Bollard)
      # - creates docker->podman symlink
      # - sets DOCKER_HOST
      podmanSetup = ''
        mkdir -p /etc/containers
        echo '{"default":[{"type":"insecureAcceptAnything"}]}' > /etc/containers/policy.json

        # No /dev/fuse on garnix, so fuse-overlayfs won't work.
        # Native overlay-on-overlay also fails. Use vfs driver.
        printf '[storage]\ndriver = "vfs"\n' > /etc/containers/storage.conf

        # Cgroup subtree_control is read-only on garnix. Disable cgroup management.
        printf '[containers]\ncgroups = "disabled"\n' > /etc/containers/containers.conf

        export DOCKER_HOST="unix:///run/podman/podman.sock"

        mkdir -p /run/podman
        podman system service --time=0 "$DOCKER_HOST" &
        PODMAN_PID=$!
        trap 'kill "$PODMAN_PID" 2>/dev/null || true' EXIT
        for _i in $(seq 1 10); do
          if [ -S /run/podman/podman.sock ]; then
            break
          fi
          sleep 1
        done

        mkdir -p /tmp/docker-compat/bin
        ln -sf "$(command -v podman)" /tmp/docker-compat/bin/docker
        export PATH="/tmp/docker-compat/bin:$PATH"
      '';

      podmanRuntimeInputs = with pkgs; [
        podman
        docker-compose
        coreutils
      ];

      mkTestApp =
        testName:
        let
          testBinary = self'.packages."test-psyche-decentralized-testing-integration_tests";
          validatorImage = self'.packages.docker-psyche-solana-test-validator;
          clientImage = self'.packages.docker-psyche-solana-test-client-no-python;

          script = pkgs.writeShellApplication {
            name = "solana-test-${testName}";
            runtimeInputs =
              podmanRuntimeInputs
              ++ (with pkgs; [
                inputs.solana-pkgs.packages.${system}.solana
                self'.packages.run-manager
                testBinary
                gnugrep
              ]);
            text = ''
              ${podmanSetup}

              echo "loading images"
              ${validatorImage} | podman load
              ${clientImage} | podman load

              # Use host networking (garnix lacks CAP_NET_ADMIN for bridges)
              export DOCKER_NETWORK=host

              echo "running test from repo root"
              cd architectures/decentralized/testing
              test-psyche-decentralized-testing-integration_tests --nocapture "${testName}"
            '';
          };
        in
        {
          type = "app";
          program = lib.getExe script;
        };

      # Debug/probe action to test podman + compose on garnix
      probeApp =
        let
          script = pkgs.writeShellApplication {
            name = "probe-garnix-env";
            runtimeInputs = podmanRuntimeInputs;
            text = ''
              ${podmanSetup}

              echo "=== PODMAN + COMPOSE TEST ==="

              echo "--- loading validator image ---"
              ${self'.packages.docker-psyche-solana-test-validator} | podman load
              podman images

              echo "--- loading client image ---"
              ${self'.packages.docker-psyche-solana-test-client-no-python} | podman load
              podman images

              echo "--- testing compose up (validator only, host network) ---"
              cd docker/test
              podman compose -f docker-compose.host-network.yml up -d --wait psyche-solana-test-validator 2>&1
              podman compose -f docker-compose.host-network.yml ps 2>&1
              podman compose -f docker-compose.host-network.yml logs psyche-solana-test-validator 2>&1 | tail -30
              podman compose -f docker-compose.host-network.yml down 2>&1

              echo "=== END ==="
            '';
          };
        in
        {
          type = "app";
          program = lib.getExe script;
        };

      dockerhubPasswordAge = ../secrets/dockerhub-password.age;

      mkDockerPushApp =
        {
          repository,
          dockerImage,
        }:
        let
          script = pkgs.writeShellApplication {
            name = "push-docker-${repository}";
            runtimeInputs = with pkgs; [
              skopeo
              age
            ];
            text = ''
              # only push on main branch
              if [ "''${GARNIX_BRANCH:-}" != "main" ]; then
                echo "Not on main branch (GARNIX_BRANCH=''${GARNIX_BRANCH:-}), skipping push"
                exit 0
              fi

              if [ -z "''${GARNIX_ACTION_PRIVATE_KEY_FILE:-}" ]; then
                echo "GARNIX_ACTION_PRIVATE_KEY_FILE not set, cannot decrypt credentials"
                exit 1
              fi

              REGISTRY_PASSWORD=$(age -d -i "$GARNIX_ACTION_PRIVATE_KEY_FILE" ${dockerhubPasswordAge})
              REGISTRY_USERNAME="arilotter"

              # stream image directly to registry
              ${dockerImage} | skopeo copy \
                --dest-creds "$REGISTRY_USERNAME:$REGISTRY_PASSWORD" \
                docker-archive:/dev/stdin \
                "docker://${repository}:latest"
            '';
          };
        in
        {
          type = "app";
          program = lib.getExe script;
        };

      testApps = builtins.listToAttrs (
        map (testName: {
          name = "solana-test-${testName}";
          value = mkTestApp testName;
        }) solanaTests
      );

      pushApps = {
        push-docker-solana-client = mkDockerPushApp {
          repository = "nousresearch/psyche-client";
          dockerImage = self'.packages.docker-psyche-solana-client;
        };
        push-docker-gateway = mkDockerPushApp {
          repository = "nousresearch/psyche-gateway-node";
          dockerImage = self'.packages.docker-psyche-gateway-node;
        };
      };
    in
    lib.optionalAttrs (system == "x86_64-linux") {
      apps = testApps // pushApps // { probe-garnix-env = probeApp; };
    };
}
