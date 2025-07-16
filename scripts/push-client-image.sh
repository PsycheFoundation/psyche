# Script for pushing the client image to Docker Hub during the CI pipeline.
set -eu

IMAGE_PATH=$(cat image-path.txt)
"$IMAGE_PATH" | docker load
IMAGE_ID=$(docker images -q | head -n1)
docker tag $IMAGE_ID $DOCKER_REPOSITORY:latest
docker push $DOCKER_REPOSITORY
