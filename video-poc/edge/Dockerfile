FROM python:3.12-slim

# Compatibility manifest — written into the image at build time so
# the running container can self-report which edge protocol it speaks
# and which server min-version it expects. Defaults are appropriate
# for local dev builds; CI overrides these via --build-arg.
ARG EDGE_VERSION=dev
ARG EDGE_GIT_SHA=unknown
ARG EDGE_BUILT_AT=unknown
ARG EDGE_PROTOCOL_VERSION=1
ARG EDGE_MIN_SERVER_VERSION=0.0.0

# Cosign version bundled into the image. Pinned for reproducibility;
# update intentionally rather than chasing latest. Tag check upstream
# on a new release before bumping.
ARG COSIGN_VERSION=2.4.1

WORKDIR /app

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    YOLO_CONFIG_DIR=/app/.ultralytics

# OpenCV-headless needs a couple of shared libs for video decode + ffmpeg
# backend. curl + ca-certificates + gnupg support both the cosign download
# below AND fetching Docker's apt key.
#
# Why not just `apt install docker.io docker-compose-plugin` in one step:
# Debian main ships `docker.io` (legacy) but does NOT ship the compose v2
# plugin. The update-agent invokes `docker compose` (v2), so we have to
# pull the CLI + plugin from Docker's official repo. Following Docker's
# documented Debian install path:
#   https://docs.docker.com/engine/install/debian/
RUN apt-get update && apt-get install -y --no-install-recommends \
        libgl1 \
        libglib2.0-0 \
        ffmpeg \
        ca-certificates \
        curl \
        gnupg \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg \
        -o /etc/apt/keyrings/docker.asc \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && . /etc/os-release \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian ${VERSION_CODENAME} stable" \
        > /etc/apt/sources.list.d/docker.list \
    && apt-get update && apt-get install -y --no-install-recommends \
        docker-ce-cli \
        docker-compose-plugin \
    && rm -rf /var/lib/apt/lists/*

# Cosign — installed as a static binary so the update-agent can
# `cosign verify ghcr.io/...:<tag>` before pulling a new image.
# Multi-arch friendly: dpkg --print-architecture picks amd64/arm64
# so the same Dockerfile works on Jetson + x86 boxes.
RUN ARCH="$(dpkg --print-architecture)" && \
    curl -fsSL -o /usr/local/bin/cosign \
        "https://github.com/sigstore/cosign/releases/download/v${COSIGN_VERSION}/cosign-linux-${ARCH}" && \
    chmod +x /usr/local/bin/cosign

COPY requirements.txt .
RUN pip install -r requirements.txt

# Bake the YOLO11 nano weights into the image so first-boot doesn't need
# network access to ultralytics-cdn. Ends up at /app/yolo11n.pt — matches the
# default YOLO_MODEL=yolo11n.pt env var in worker/camera.py.
RUN python -c "from ultralytics import YOLO; YOLO('yolo11n.pt')"

COPY worker ./worker

# Provision + main wrapper. Runs `worker.provision` first so
# greenfield boxes self-bootstrap before `worker.main` tries to read
# EDGE_BOX_ID/EDGE_BOX_TOKEN. See worker/entrypoint.sh for the full
# rationale; the script is idempotent so already-onboarded boxes pay
# only the short-circuit cost on container start.
RUN chmod +x /app/worker/entrypoint.sh

# Compatibility manifest. Read by the running container so the server's
# /fleet/announce handler can eventually reject too-old workers (see
# internal-docs/edge-image-releases.md follow-ups).
RUN mkdir -p /opt/oxy-edge && \
    printf '{"edge_version":"%s","git_sha":"%s","built_at":"%s","protocol_version":%s,"min_server_version":"%s"}\n' \
        "$EDGE_VERSION" "$EDGE_GIT_SHA" "$EDGE_BUILT_AT" "$EDGE_PROTOCOL_VERSION" "$EDGE_MIN_SERVER_VERSION" \
        > /opt/oxy-edge/compatibility.json

RUN mkdir -p /var/lib/outbox /app/.ultralytics

# OCI labels — surfaced in `docker inspect` and on GHCR's package page.
LABEL org.opencontainers.image.title="oxy-edge" \
      org.opencontainers.image.source="https://github.com/oxy-hq/oxy" \
      org.opencontainers.image.licenses="Proprietary" \
      org.opencontainers.image.version="${EDGE_VERSION}" \
      org.opencontainers.image.revision="${EDGE_GIT_SHA}"

# Greenfield-friendly entrypoint: run `worker.provision` (idempotent)
# then exec whatever CMD docker passed us. Both the edge worker and
# the update agent share this wrapper — they only need different
# CMDs, set per-service in docker-compose.yml. See worker/entrypoint.sh.
ENTRYPOINT ["/app/worker/entrypoint.sh"]
CMD ["python", "-m", "worker.main"]
