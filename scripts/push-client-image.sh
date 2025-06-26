set -eu

DOCKER_REPOSITORY="ignacioavecilla/psyche-solana-client"
DOCKER_USERNAME="ignacioavecilla"

echo "dckr_pat_LS668OPTwTAXR3b3pCLJit5b1O0" | docker login -u "$DOCKER_USERNAME" --password-stdin
nix build .#docker-psyche-solana-client --out-link nix-results/docker-psyche-solana-client
nix-results/docker-psyche-solana-client | docker load
docker push "$DOCKER_REPOSITORY"
