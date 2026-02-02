# Build stage
FROM rust:1.84-bookworm AS builder

# Install dependencies for dx and wasm
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    binaryen \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

# Install dioxus-cli from source (with limited parallelism to reduce memory)
RUN CARGO_BUILD_JOBS=2 cargo install dioxus-cli@0.7.1 --locked && \
    dx --version

# Create app directory
WORKDIR /app

# Copy everything (simpler approach for now)
COPY . .

# Debug: check project structure
RUN ls -la && ls -la packages/

# Build the fullstack app
RUN RUST_BACKTRACE=1 dx bundle --fullstack --release --debug-symbols=false 2>&1 | tee /tmp/build.log; \
    exit_code=$?; \
    if [ $exit_code -ne 0 ]; then \
        echo "=== Build failed with exit code $exit_code ==="; \
        cat /tmp/build.log; \
        exit $exit_code; \
    fi

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built artifacts
COPY --from=builder /app/target/dx/web/release/web/web ./web
COPY --from=builder /app/target/dx/web/release/web/public ./public

# Expose port
EXPOSE 8080

# Set environment variables
ENV RUST_LOG=info
ENV PORT=8080

# Run the server
CMD ["./web"]
