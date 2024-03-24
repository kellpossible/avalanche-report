#!/bin/sh

# Build and publish the images to dockerhub.

./docker-build.sh
docker push "lfrisken/avalanche-report:latest"
docker push "lfrisken/avalanche-report:$(./git-tag.sh)"

