set -eu

DOCKER_REPOSITORY="ignacioavecilla/psyche-solana-client"
DOCKER_USERNAME="ignacioavecilla"

echo "dckr_pat_LS668OPTwTAXR3b3pCLJit5b1O0" | docker login -u "$DOCKER_USERNAME" --password-stdin
IMAGE_PATH=$(cat image-path.txt)
docker load < "$IMAGE_PATH"
IMAGE_ID=$(docker images -q | head -n1)
docker tag "$IMAGE_ID" ignacioavecilla/psyche-solana-client:latest
docker push "$DOCKER_REPOSITORY"
