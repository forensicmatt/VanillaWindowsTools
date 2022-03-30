FROM rustlang/rust@sha256:37facccba0bc0c8f7f6301ecc8b3033e4003d40bfb427bb3bfecfaf53ce75228 AS builder
WORKDIR /src/vanilla
COPY . .
RUN cargo build --release

FROM builder AS service
CMD ./target/release/vanilla_service -s ./VanillaWindowsReference -i /vanilla-index -a 0.0.0.0

