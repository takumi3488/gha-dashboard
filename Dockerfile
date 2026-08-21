FROM rust:1.97-slim-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y pkg-config libssl-dev
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:latest@sha256:e5d81ddde149641e2a9ba55be4545bc125c67de07508b03ba4c22e6eb0ded5aa
WORKDIR /usr/src/app
COPY --from=builder /usr/src/app/target/release/gha-dashboard /usr/local/bin/gha-dashboard
CMD ["gha-dashboard"]