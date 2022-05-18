FROM rustlang/rust:nightly-buster@sha256:d11c27a37b3c07d99d244014ba058b9e8e2fda4596a516f9574e513eccbb09f2 AS builder

WORKDIR /source
RUN USER=root cargo new service
WORKDIR /source/service

# copy over the manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache dependencies
RUN cargo build --release

# copy your source tree
COPY ./src ./src

# Rebuild with newest source
RUN cargo build --release

FROM debian:buster-slim AS service
RUN apt update && apt install -y libssl-dev ca-certificates
WORKDIR /vanilla
COPY --from=builder /source/service/target/release/vanilla_service ./vanilla_service
CMD ./vanilla_service -i ./index -s ./data -a 0.0.0.0
