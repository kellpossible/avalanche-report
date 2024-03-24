#!/bin/sh

# Build the images that are published to dockerhub.

docker build --tag "lfrisken/avalanche-report:$(./git-tag.sh)" --tag "lfrisken/avalanche-report:latest"  .
