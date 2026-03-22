FROM rust:1.84-slim AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/intellisphere-sphere /sphere
COPY --from=builder /app/config /config

USER nobody:nobody

ENTRYPOINT ["/sphere"]
