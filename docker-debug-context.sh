#!/bin/sh

# A script for debugging .dockerignore, to check what's in the build context.

# Define environment variables for the binary and its download URL
docker build --no-cache --progress plain --file - . <<EOF
FROM debian

# Install necessary tools to download and extract
RUN apt-get update && apt-get install -y curl tar

# Download the binary and extract it
RUN curl -fsSL "https://github.com/solidiquis/erdtree/releases/download/v3.1.2/erd-v3.1.2-x86_64-unknown-linux-gnu.tar.gz" | tar -xz -C /usr/local/bin/

COPY . /build-context
RUN erd --color force --sort rsize --human /build-context
EOF
