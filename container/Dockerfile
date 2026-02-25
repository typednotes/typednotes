# Build stage
FROM rust:bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    binaryen \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

# Install Node.js (for Tailwind CSS build)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install cargo-binstall and dioxus-cli (pre-built binary)
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash && \
    cargo binstall dioxus-cli --no-confirm

WORKDIR /app

# Copy everything
COPY . .

# Build Tailwind CSS
RUN npm install tailwindcss @tailwindcss/cli && \
    npx @tailwindcss/cli -i ./input.css -o ./packages/web/assets/tailwind.css --minify

# Build the fullstack app
RUN dx bundle --fullstack --release --debug-symbols=false

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built artifacts
COPY --from=builder /app/target/dx/web/release/web/web ./web
COPY --from=builder /app/target/dx/web/release/web/public ./public

EXPOSE 8080

ENV RUST_LOG=info
ENV PORT=8080

CMD ["./web"]
