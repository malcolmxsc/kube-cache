# STAGE 1: Build the Binary
# We use the official Rust image to compile our code
FROM rust:1.81 as builder

# Create a dummy project to cache dependencies
# (This prevents re-downloading crates every time you change one line of code)
WORKDIR /usr/src/kube-cache
COPY Cargo.toml Cargo.lock ./
# Create a dummy main file to force cargo to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Now copy the REAL source code
COPY src ./src
# Touch the main file so cargo knows to rebuild it
RUN touch src/main.rs
RUN cargo build --release

# STAGE 2: The Runtime Image
# We use "Distroless" (Google's optimized runtime images)
# It contains NO shell, NO package manager, just the bare minimum to run Rust.
# This is huge for security.
FROM gcr.io/distroless/cc-debian12

# Copy the binary from the builder stage
COPY --from=builder /usr/src/kube-cache/target/release/kube-cache /app/kube-cache

# Run it
ENTRYPOINT ["/app/kube-cache"]