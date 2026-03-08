# Multi-stage build for UDE (Universal Developer Engine)

# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.toml
COPY crates/ crates/

# Build dependencies (cached layer)
RUN mkdir -p crates/gateway/src && \
    echo "fn main() {}" > crates/gateway/src/main.rs && \
    cargo build --release && \
    rm -rf crates/gateway/src

# Copy source code
COPY crates/ crates/

# Build application
RUN cargo build --release --bin gateway

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/gateway /usr/local/bin/gateway

# Create non-root user
RUN useradd -m -u 1000 ude && \
    chown -R ude:ude /app

USER ude

# Expose port
EXPOSE 4122

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:4122/v1/api/health || exit 1

# Run
ENTRYPOINT ["gateway"]
CMD ["--port", "4122"]
