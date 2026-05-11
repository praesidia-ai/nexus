# Multi-stage build for the Nexus server
FROM rust:1.82-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release --bin nexus-server 2>/dev/null || true
RUN cargo build --release --bin nexus-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/nexus-server /usr/local/bin/nexus-server
RUN useradd --system --uid 10001 --create-home --home-dir /home/nexus --shell /usr/sbin/nologin nexus \
 && mkdir -p /data \
 && chown -R nexus:nexus /data
ENV NEXUS_DATA_DIR=/data
ENV NEXUS_PORT=8020
USER nexus
EXPOSE 8020
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD curl -f http://localhost:8020/health || exit 1
CMD ["nexus-server"]
