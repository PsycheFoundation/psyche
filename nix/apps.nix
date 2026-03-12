# Garnix apps for Docker image pushing
{ ... }:
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
      apps = pushApps;
    };
}
