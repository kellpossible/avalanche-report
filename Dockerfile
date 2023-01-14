####################################################################################################
## Builder
####################################################################################################
FROM rust:latest AS builder

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev
RUN update-ca-certificates

WORKDIR /avalanche-report

COPY ./ .

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
