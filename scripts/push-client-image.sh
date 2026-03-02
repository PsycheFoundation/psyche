# Script for pushing the client image to Docker Hub during the CI pipeline.
# Uses skopeo to stream the image directly to the registry without loading
# into Docker's local storage, avoiding double disk usage.
set -eu

IMAGE_PATH=$(cat image-path.txt)
"$IMAGE_PATH" | skopeo copy \
    --dest-creds "${REGISTRY_USERNAME}:${REGISTRY_PASSWORD}" \
    docker-archive:/dev/stdin \
    "docker://${DOCKER_REPOSITORY}:latest"
