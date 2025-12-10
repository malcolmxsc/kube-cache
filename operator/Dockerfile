# STAGE 1: Build on Old Stable Linux (Bullseye)
FROM rust:bullseye AS builder

# Install build tools
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    perl \
    cmake \
    clang

# Create a dummy project to cache dependencies
WORKDIR /usr/src/kube-cache
COPY operator/Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Copy the REAL source code
COPY operator/src ./src
RUN touch src/main.rs
RUN cargo build --release

# STAGE 2: Runtime on Newer Linux (Bookworm)
FROM debian:bookworm-slim

# Install CA certs
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the binary
COPY --from=builder /usr/src/kube-cache/target/release/kube-cache /app/kube-cache

# Run it
ENTRYPOINT ["/app/kube-cache"]