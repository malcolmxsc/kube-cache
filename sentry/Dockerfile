# STAGE 1: Build the eBPF Bytecode (Requires Nightly)
FROM rustlang/rust:nightly AS bpf-builder

# Install rust-src (Required for -Z build-std=core)
RUN rustup component add rust-src

# Install bpf-linker
RUN cargo install bpf-linker

WORKDIR /app

# Copy the whole workspace (easier to handle dependencies)
COPY Cargo.toml Cargo.lock ./
COPY operator ./operator
COPY sentry ./sentry

# Build the eBPF program
# We target the specific crate 'sentry-ebpf'
WORKDIR /app/sentry/sentry-ebpf
RUN cargo build --release --target=bpfel-unknown-none -Z build-std=core

# STAGE 2: Build the Userspace Binary (Requires Stable/Bullseye)
FROM rust:bullseye AS userspace-builder

WORKDIR /app

# Copy the workspace again (fresh start for stable)
COPY Cargo.toml Cargo.lock ./
COPY operator ./operator
COPY sentry ./sentry

# Copy the compiled BPF bytecode from Stage 1 to the location expected by main.rs
# main.rs expects: "../../target/bpfel-unknown-none/release/sentry" relative to "sentry/sentry/src"
# This resolves to: /app/sentry/target/bpfel-unknown-none
COPY --from=bpf-builder /app/target/bpfel-unknown-none /app/sentry/target/bpfel-unknown-none

# Build the userspace binary
WORKDIR /app
RUN cargo build --release -p sentry

# STAGE 3: Runtime
FROM gcr.io/distroless/cc-debian12

# Copy the binary
COPY --from=userspace-builder /app/target/release/sentry /app/sentry

# Privileged mode is handled by K8s, but we need to run as root (default in distroless)
ENTRYPOINT ["/app/sentry"]
