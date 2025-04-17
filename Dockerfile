# ---------------------------------------------------------------------------------
# NOTE: This Dockerfile is NOT necessary for the development process right now.
# It is reserved for potential usage in the future.
# ---------------------------------------------------------------------------------

# Base image for Rust and Cargo Chef
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# Stage 1: Dependency planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build the web application
FROM node:lts AS web-builder
WORKDIR /app

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web-app/package.json web-app/pnpm-lock.yaml ./web-app/
RUN corepack enable && corepack prepare --activate && pnpm install

COPY web-app/ ./web-app/
RUN pnpm -C web-app build

# Stage 3: Build the Rust application
FROM chef AS rust-builder

RUN apt-get update && \
    apt-get install -y protobuf-compiler ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
COPY --from=web-builder /app/web-app/dist /app/crates/core/dist
RUN cargo build --release

# Stage 4: Runtime image
FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates tini && \
    rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/oxy /usr/local/bin

# Set tini as the entrypoint
ENTRYPOINT ["/usr/bin/tini", "--"]

# Default command
EXPOSE 3000
CMD ["oxy", "serve", "--port", "3000"]
