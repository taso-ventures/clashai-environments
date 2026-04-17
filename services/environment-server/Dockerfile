# Build stage
FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release --bin environment-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 65532 app && useradd -u 65532 -g app -s /usr/sbin/nologin app

COPY --from=builder --chown=app:app \
    /app/target/release/environment-server \
    /usr/local/bin/environment-server

COPY --from=builder --chown=app:app \
    /app/services/environment-server/static \
    /app/static

WORKDIR /app
USER app:app

EXPOSE 8080

ENV RUST_LOG=info
ENV PORT=8080
ENV STATIC_DIR=/app/static
ENV WS_CAPACITY=256

CMD ["environment-server"]
