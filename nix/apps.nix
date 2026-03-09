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

      mkTestApp =
        testName:
        let
          testBinary = self'.packages."test-psyche-decentralized-testing-integration_tests";
          validatorImage = self'.packages.docker-psyche-solana-test-validator;
          clientImage = self'.packages.docker-psyche-solana-test-client-no-python;

          # Inner script runs as non-root user with rootless Docker
          innerScript = pkgs.writeShellApplication {
            name = "solana-test-${testName}-inner";
            runtimeInputs = with pkgs; [
              docker
              just
              inputs.solana-pkgs.packages.${system}.solana
              self'.packages.run-manager
              testBinary
              coreutils
              gnugrep
            ];
            text = ''
              echo "starting rootless docker daemon"
              XDG_RUNTIME_DIR=$(mktemp -d)
              export XDG_RUNTIME_DIR
              HOME=$(mktemp -d)
              export HOME
              dockerd-rootless &

              DOCKER_HOST="unix://$XDG_RUNTIME_DIR/docker.sock"
              export DOCKER_HOST

              echo "waiting for docker to be ready"
              for i in $(seq 1 30); do
                if docker info >/dev/null 2>&1; then
                  break
                fi
                if [ "$i" = "30" ]; then
                  echo "Docker failed to start"
                  exit 1
                fi
                sleep 1
              done

              echo "loading images from nix store"
              ${validatorImage} | docker load
              ${clientImage} | docker load

              echo "running test from repo root"
              cd architectures/decentralized/testing
              test-psyche-decentralized-testing-integration_tests --nocapture "${testName}"
            '';
          };

          # Outer script creates a non-root user and runs the inner script via su,
          # because dockerd-rootless refuses to run as root.
          script = pkgs.writeShellApplication {
            name = "solana-test-${testName}";
            runtimeInputs = with pkgs; [
              shadow
              su
              coreutils
            ];
            text = ''
              echo "setting up non-root user for rootless docker"

              # Create a test user
              useradd -m testuser 2>/dev/null || true

              # Set up subuid/subgid ranges for rootless Docker
              echo "testuser:100000:65536" > /etc/subuid
              echo "testuser:100000:65536" > /etc/subgid

              # newuidmap/newgidmap need to be setuid for rootlesskit.
              # Copy them from the nix store and set the suid bit.
              cp ${pkgs.shadow}/bin/newuidmap /usr/local/bin/newuidmap
              cp ${pkgs.shadow}/bin/newgidmap /usr/local/bin/newgidmap
              chmod u+s /usr/local/bin/newuidmap /usr/local/bin/newgidmap

              # Run the inner script as the test user, preserving PATH for nix store access
              su testuser -s /bin/sh -c 'export PATH="/usr/local/bin:$PATH"; exec ${lib.getExe innerScript}'
            '';
          };
        in
        {
          type = "app";
          program = lib.getExe script;
        };

      # Debug/probe action to understand the garnix VM environment
      probeApp =
        let
          script = pkgs.writeShellApplication {
            name = "probe-garnix-env";
            excludeShellChecks = [
              "SC2310"
            ];
            runtimeInputs = with pkgs; [
              coreutils
              docker
              shadow
              su
              util-linux
            ];
            text = ''
              set +e
              echo "=== ROOTLESS DOCKER SETUP TEST ==="

              echo "--- 1. create user ---"
              useradd -m testuser 2>&1; echo "exit: $?"

              echo "--- 2. subuid/subgid ---"
              echo "testuser:100000:65536" > /etc/subuid 2>&1; echo "subuid exit: $?"
              echo "testuser:100000:65536" > /etc/subgid 2>&1; echo "subgid exit: $?"
              cat /etc/subuid /etc/subgid

              echo "--- 3. suid newuidmap/newgidmap ---"
              mkdir -p /usr/local/bin
              cp ${pkgs.shadow}/bin/newuidmap /usr/local/bin/newuidmap 2>&1; echo "cp newuidmap: $?"
              cp ${pkgs.shadow}/bin/newgidmap /usr/local/bin/newgidmap 2>&1; echo "cp newgidmap: $?"
              chmod u+s /usr/local/bin/newuidmap /usr/local/bin/newgidmap 2>&1; echo "chmod suid: $?"
              ls -la /usr/local/bin/new*idmap

              echo "--- 4. unshare test ---"
              unshare --user --map-root-user echo "unshare user ns works" 2>&1; echo "exit: $?"

              echo "--- 5. unshare as testuser ---"
              su testuser -s /bin/sh -c 'id; unshare --user echo "user unshare works"' 2>&1; echo "exit: $?"

              echo "--- 6. rootlesskit test ---"
              su testuser -s /bin/sh -c 'export XDG_RUNTIME_DIR=$(mktemp -d); export HOME=$(mktemp -d); export PATH=/usr/local/bin:$PATH:${pkgs.docker}/bin; rootlesskit echo "rootlesskit works"' 2>&1; echo "exit: $?"

              echo "--- 7. cgroups ---"
              cat /proc/self/cgroup 2>&1 || echo "no cgroup"
              find /sys/fs/cgroup/ -maxdepth 1 2>/dev/null || echo "no cgroups dir"

              echo "--- 8. attempt dockerd-rootless ---"
              su testuser -s /bin/sh -c '
                export XDG_RUNTIME_DIR=$(mktemp -d)
                export HOME=$(mktemp -d)
                export PATH=/usr/local/bin:$PATH:${pkgs.docker}/bin
                timeout 15 dockerd-rootless 2>&1 &
                PID=$!
                export DOCKER_HOST="unix://$XDG_RUNTIME_DIR/docker.sock"
                for i in $(seq 1 10); do
                  if docker info >/dev/null 2>&1; then
                    echo "DOCKER IS RUNNING"
                    docker info 2>&1
                    break
                  fi
                  sleep 1
                done
                kill $PID 2>/dev/null || true
                wait $PID 2>/dev/null || true
              ' 2>&1; echo "exit: $?"

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
