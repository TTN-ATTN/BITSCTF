# Build stage
FROM rust:1.83-slim AS builder
WORKDIR /build
COPY Cargo.toml .
COPY src/ src/
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/securevault-proxy /usr/local/bin/proxy
EXPOSE 80
CMD ["proxy"]
