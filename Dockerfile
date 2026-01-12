# ---------------------------------------------------------------------------------
# NOTE: This Dockerfile is NOT necessary for the development process right now.
# It is reserved for potential usage in the future.
# ---------------------------------------------------------------------------------

# Base image for Rust and Cargo Chef
FROM lukemathwalker/cargo-chef:latest-rust-1.92.0-bookworm AS chef
WORKDIR /app

# Stage 1: Dependency planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build the web application
FROM node:24-slim AS web-builder
WORKDIR /app

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web-app/package.json ./web-app/
RUN corepack enable && corepack prepare --activate && pnpm install

COPY web-app/ ./web-app/
ARG VITE_SENTRY_DSN
ENV VITE_SENTRY_DSN=$VITE_SENTRY_DSN
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
    apt-get install -y ca-certificates tini git && \
    rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/oxy /usr/local/bin

# Directory for persistent app data inside the container
ENV OXY_STATE_DIR=/var/lib/oxy/data
RUN mkdir -p ${OXY_STATE_DIR} && chown -R root:root /var/lib/oxy
VOLUME ["${OXY_STATE_DIR}"]

# Set tini as the entrypoint
ENTRYPOINT ["/usr/bin/tini", "--"]

# Default command
EXPOSE 3000
CMD ["oxy", "serve", "--port", "3000"]

# Stage 5: Semantic Engine (Cube.js) - Optional deployment target
FROM cubejs/cube:v1.3.81 AS semantic-engine

# Install the oxy binary to generate Cube.js configuration
COPY --from=rust-builder /app/target/release/oxy /usr/local/bin/oxy

# Set working directory for the project
WORKDIR /app

# Copy project files needed for semantic layer generation
# These will be used to generate the Cube.js schema at runtime or build time
# Uncomment and adjust these lines based on your project structure:
# COPY semantics/ /app/semantics/
# COPY config.yml /app/config.yml

# Environment variables for Cube.js
ENV CUBEJS_DEV_MODE=true \
    CUBEJS_DB_TYPE=duckdb \
    NODE_ENV=development

# Expose Cube.js default port
EXPOSE 4000

# Wrapper script to generate config and start Cube.js
COPY <<'EOF' /usr/local/bin/start-semantic-engine.sh
#!/bin/sh
set -e

# Use environment variable or current working directory
WORK_DIR="${OXY_WORK_DIR:-$(pwd)}"
OUTPUT_DIR="${OXY_CUBE_OUTPUT_DIR:-/cube/conf}"

# Generate Cube.js configuration from semantic layer
if [ -d "${WORK_DIR}/semantics" ]; then
    echo "Generating Cube.js configuration from semantic layer..."
    echo "Working directory: ${WORK_DIR}"
    echo "Output directory: ${OUTPUT_DIR}"
    cd "${WORK_DIR}" && oxy prepare-semantic-engine --force --output-dir "${OUTPUT_DIR}"
else
    echo "Warning: No semantics directory found in ${WORK_DIR}. Using existing configuration."
fi

echo "📦 Cube.js configuration is ready for deployment"
echo "You can now run Cube.js natively or in a container using the generated config"
echo "Starting Cube.js server..."
echo "Schema path: ${CUBEJS_SCHEMA_PATH}"
echo "Dev mode: ${CUBEJS_DEV_MODE}"
echo ""
echo "Database configuration:"
echo "  CUBEJS_DB_TYPE: ${CUBEJS_DB_TYPE}"
echo "  CUBEJS_DB_URL: ${CUBEJS_DB_URL}"
echo "  CUBEJS_DB_HOST: ${CUBEJS_DB_HOST}"
echo "  CUBEJS_DB_PORT: ${CUBEJS_DB_PORT}"
echo "  CUBEJS_DB_NAME: ${CUBEJS_DB_NAME}"
echo "  CUBEJS_DB_USER: ${CUBEJS_DB_USER}"
echo ""

# Change to output directory where cube.js and model files are located
cd "${OUTPUT_DIR}"

# Use the cubejs command from the base image
# The base image has cubejs CLI available in PATH
exec cubejs server
EOF

RUN chmod +x /usr/local/bin/start-semantic-engine.sh

CMD ["/usr/local/bin/start-semantic-engine.sh"]