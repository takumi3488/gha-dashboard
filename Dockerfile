FROM rust:1.87-slim-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y pkg-config libssl-dev
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:latest
WORKDIR /usr/src/app
COPY --from=builder /usr/src/app/target/release/gha-dashboard /usr/local/bin/gha-dashboard
CMD ["gha-dashboard"]