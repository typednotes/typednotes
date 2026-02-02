# Build stage
FROM rust:1.84-bookworm AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    binaryen \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

# Install wasm-bindgen-cli
RUN cargo install wasm-bindgen-cli@0.2.100 --locked

WORKDIR /app

# Copy everything
COPY . .

# Build the server
RUN cargo build --release -p web --features server

# Build the WASM client
RUN cargo build --release -p web --target wasm32-unknown-unknown --features web

# Process WASM with wasm-bindgen
RUN mkdir -p dist/assets && \
    wasm-bindgen \
        --out-dir dist/assets \
        --out-name app \
        --target web \
        target/wasm32-unknown-unknown/release/web.wasm

# Optimize WASM
RUN wasm-opt -Oz -o dist/assets/app_bg.wasm dist/assets/app_bg.wasm || true

# Copy static assets
RUN cp -r packages/web/assets/* dist/assets/ 2>/dev/null || true

# Create index.html
RUN cat > dist/index.html << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>TypedNotes</title>
    <link rel="icon" href="/assets/favicon.ico">
    <link rel="stylesheet" href="/assets/main.css">
</head>
<body>
    <div id="main"></div>
    <script type="module">
        import init from '/assets/app.js';
        init();
    </script>
</body>
</html>
EOF

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy server binary
COPY --from=builder /app/target/release/web ./web

# Copy client assets
COPY --from=builder /app/dist ./public

EXPOSE 8080

ENV RUST_LOG=info
ENV PORT=8080

CMD ["./web"]
