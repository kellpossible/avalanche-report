####################################################################################################
## Chef
####################################################################################################
FROM rust:latest AS chef

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev nodejs npm curl
RUN update-ca-certificates

RUN curl --location https://github.com/casey/just/releases/download/1.13.0/just-1.13.0-x86_64-unknown-linux-musl.tar.gz \
  --output /tmp/just-1.13.0-x86_64-unknown-linux-musl.tar.gz &&\
echo "f76fce93a71686f6aa6b2db1a39184e736f9ac8248c0489e003c617b49eb2676  /tmp/just-1.13.0-x86_64-unknown-linux-musl.tar.gz" | sha256sum -c &&\
mkdir /tmp/just &&\
    tar --directory=/tmp/just -xvf /tmp/just-1.13.0-x86_64-unknown-linux-musl.tar.gz &&\
    cp /tmp/just/just /usr/local/bin
RUN cargo install cargo-chef

WORKDIR /avalanche-report

####################################################################################################
## Planner
####################################################################################################
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

####################################################################################################
## Builder
####################################################################################################
FROM chef AS builder

# Shenanigans in order to cache npm install
COPY package.json .
COPY package-lock.json .
RUN npm install

COPY --from=planner /avalanche-report/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook -p migrations --target x86_64-unknown-linux-musl --release --recipe-path recipe.json
RUN cargo chef cook --target x86_64-unknown-linux-musl --release --recipe-path recipe.json

COPY . .

RUN just tailwind

# For sqlx macro
ARG DATABASE_URL="sqlite://data/db.sqlite3"
RUN cargo run -p migrations --target x86_64-unknown-linux-musl --release
RUN cargo build --target x86_64-unknown-linux-musl --release

####################################################################################################
## Final image
####################################################################################################
FROM alpine AS deploy

WORKDIR /avalanche-report

# Copy our build
COPY --from=builder /avalanche-report/target/x86_64-unknown-linux-musl/release/avalanche-report ./

STOPSIGNAL SIGINT
CMD ["/avalanche-report/avalanche-report"]
