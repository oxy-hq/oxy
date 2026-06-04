# video-poc — Makefile entrypoints for the simulation environment.
# All targets are thin wrappers over docker compose + pumba; no hidden state.

COMPOSE        := docker compose
EDGE_NET       := video-poc_video-poc          # bridge network name
N              ?= 1
SVC            ?= edge
PCT            ?= 30
DURATION       ?= 60s
EDGE_CONTAINER ?= video-poc-edge-1             # default first replica

.PHONY: help edge scale logs down clean check ppe-model training-frames train-ppe test-install \
        edge-vm-stop edge-vm-start edge-vm-restart edge-vm-status edge-vm-rebuild \
        chaos-disconnect chaos-loss chaos-latency

help:
	@echo "Targets:"
	@echo "  check                    — validate docker-compose.yml"
	@echo "  edge                     — start the edge stack (mediamtx + worker)"
	@echo "  scale N=3                — scale edge workers to N replicas"
	@echo "  logs SVC=edge            — tail logs for a service (mediamtx, edge)"
	@echo "  ppe-model                — download a public PPE-YOLO model into ./models/"
	@echo "  training-frames          — extract JPEG frames from MTX streams for Roboflow upload"
	@echo "  train-ppe                — train YOLOv8 locally from a Roboflow-exported dataset"
	@echo "  test-install             — run install-edge.sh end-to-end in an OrbStack VM"
	@echo "  edge-vm-stop             — suspend the OrbStack VM (frees RAM, keeps disk)"
	@echo "  edge-vm-start            — resume the OrbStack VM"
	@echo "  edge-vm-restart          — bounce just the compose stack inside the VM"
	@echo "  edge-vm-status           — show VM + container state"
	@echo "  edge-vm-rebuild          — rebuild edge+update-agent images and restart"
	@echo "  chaos-disconnect         — sever edge<->central for $${DURATION:-60s}"
	@echo "  chaos-loss PCT=30        — inject PCT% packet loss for DURATION"
	@echo "  chaos-latency MS=500     — add MS ms latency for DURATION"
	@echo "  down                     — stop everything, keep volumes"
	@echo "  clean                    — stop and wipe volumes (loses outbox + recordings)"

# ---------------------------------------------------------------------------
# Preflight checks. Fast — run these before `edge` to catch wiring bugs
# without waiting for a full container boot + traceback. The old
# `check-api` target (FastAPI import smoke test) is gone along with the
# control-api service — the control plane lives in Oxy proper now.
# ---------------------------------------------------------------------------

check:
	@$(COMPOSE) config --quiet && echo "ok docker-compose.yml"

# Drop a public PPE-YOLO model into ./models/. Idempotent — no-ops
# when the default file already exists. Pass URL=<https://…/file.pt>
# to swap the source.
ppe-model:
	@scripts/download-ppe-model.sh $(URL)

# Extract JPEG frames from MTX paths (or a passed SOURCE) for Roboflow
# upload. Tunables: INTERVAL_S, DURATION_S, MAX_FRAMES.
# Examples:
#   make training-frames                          # auto-discover from MTX
#   make training-frames SOURCE=cam-protect-01
#   make training-frames SOURCE=rtsp://10.0.0.5/live INTERVAL_S=5
training-frames:
	@scripts/extract-training-frames.sh $(SOURCE)

# Train a YOLOv8 PPE model locally from a Roboflow-exported dataset.
# Requires ROBOFLOW_API_KEY in env. Uses MPS on Apple Silicon, CUDA
# if available, CPU otherwise. Free Roboflow plan supports dataset
# export — only weight download is gated, which we bypass by training
# locally and dropping best.pt into ./models/.
# Examples:
#   make train-ppe
#   EPOCHS=100 BATCH=8 make train-ppe
train-ppe:
	@scripts/train-ppe-model.sh

# End-to-end smoke test for install-edge.sh in a throwaway OrbStack
# VM. Doubles as the per-box flow for the bearer → JWT migration.
#
# Required args via env or `make test-install OXY_URL=...`:
#   OXY_URL
#
# Optional:
#   DEVICE_ID, DEVICE_SECRET — when unset, the script auto-mints
#                              via POST /:wsid/fleet/prepared-devices.
#                              When set, those credentials are used
#                              verbatim (skip the mint round-trip).
#   MINT=1                    — force a fresh mint even when
#                              DEVICE_ID is set (e.g. previous
#                              credentials were consumed)
#   WORKSPACE_ID              — workspace UUID for the mint POST
#                              (default: nil UUID = local serve mode)
#   SITE_ID                   — site UUID the new device claims at
#                              (default: first site listed by the
#                              workspace's `/cameras/sites`)
#   OXY_AUTH_TOKEN            — operator JWT for the mint call when
#                              auth is enabled. Browser DevTools →
#                              Application → Local Storage →
#                              `auth_token`. Unset is fine for
#                              `oxy serve --local` where auth is off.
#   VM_NAME                   — OrbStack VM name (default: oxy-edge-test)
#   TS_AUTHKEY=tskey-auth-…   — Tailscale auth key. Threaded through
#                              install-edge.sh's --ts-authkey AND
#                              the auto-mint request body so the
#                              audit-logged install_command is
#                              consistent. Falls back to video-poc/.env.
#   RESET=1, KEEP=1           — VM lifecycle.
test-install:
	@scripts/test-install-script.sh \
	  $(if $(DEVICE_ID),--device-id $(DEVICE_ID),) \
	  $(if $(DEVICE_SECRET),--device-secret $(DEVICE_SECRET),) \
	  --oxy-url     "$(OXY_URL)" \
	  $(if $(VM_NAME),--vm-name $(VM_NAME),) \
	  $(if $(TS_AUTHKEY),--ts-authkey $(TS_AUTHKEY),) \
	  $(if $(MINT),--mint,) \
	  $(if $(WORKSPACE_ID),--workspace-id $(WORKSPACE_ID),) \
	  $(if $(SITE_ID),--site-id $(SITE_ID),) \
	  $(if $(OXY_AUTH_TOKEN),--auth-token $(OXY_AUTH_TOKEN),) \
	  $(if $(RESET),--reset,) \
	  $(if $(KEEP),--keep,)

# ---------------------------------------------------------------------------
# OrbStack VM controls — operate the dev edge stack without leaving the
# Makefile. VM_NAME defaults match `make test-install`'s default so the
# same VM the install harness creates is what these targets manage.
# ---------------------------------------------------------------------------

ORB_VM ?= oxy-edge-test

edge-vm-stop:
	@orb stop $(ORB_VM)

edge-vm-start:
	@orb start $(ORB_VM)
	@echo "✓ VM resumed — oxy-edge.service auto-starts via systemd"

edge-vm-restart:
	@orb run -m $(ORB_VM) -u root systemctl restart oxy-edge.service
	@echo "✓ compose stack restarted"

edge-vm-status:
	@orb list | grep -E "^NAME|$(ORB_VM)" || echo "(VM $(ORB_VM) not found — run \`make test-install\` to create)"
	@echo
	@orb run -m $(ORB_VM) -u root bash -c "cd /opt/oxy-edge && docker compose ps --format 'table {{.Service}}\t{{.Status}}'" 2>/dev/null || true

# Rebuild after worker code changes on the host. The dev compose's
# build context is a symlink into /mnt/mac/... (the host file tree),
# so just `docker compose build` picks up host-side edits — no
# `orb push` of files needed.
edge-vm-rebuild:
	@orb run -m $(ORB_VM) -u root bash -c "cd /opt/oxy-edge && docker compose build edge update-agent" \
		&& orb run -m $(ORB_VM) -u root systemctl restart oxy-edge.service \
		&& echo "✓ images rebuilt + service restarted"

edge:
	$(COMPOSE) --profile edge up -d --build

scale:
	$(COMPOSE) --profile edge up -d --build --scale edge=$(N)

logs:
	$(COMPOSE) logs -f $(SVC)

# ---------------------------------------------------------------------------
# Chaos targets — uses pumba directly against running edge containers.
# These exit when the duration elapses and the network restores itself.
# ---------------------------------------------------------------------------

chaos-disconnect:
	@echo "Severing $(EDGE_CONTAINER) from network for $(DURATION)..."
	docker run --rm -v /var/run/docker.sock:/var/run/docker.sock gaiaadm/pumba \
		netem --duration $(DURATION) --tc-image gaiadocker/iproute2 \
		loss --percent 100 $(EDGE_CONTAINER)

chaos-loss:
	@echo "Injecting $(PCT)% packet loss on $(EDGE_CONTAINER) for $(DURATION)..."
	docker run --rm -v /var/run/docker.sock:/var/run/docker.sock gaiaadm/pumba \
		netem --duration $(DURATION) --tc-image gaiadocker/iproute2 \
		loss --percent $(PCT) $(EDGE_CONTAINER)

chaos-latency:
	@echo "Adding $${MS:-500}ms latency on $(EDGE_CONTAINER) for $(DURATION)..."
	docker run --rm -v /var/run/docker.sock:/var/run/docker.sock gaiaadm/pumba \
		netem --duration $(DURATION) --tc-image gaiadocker/iproute2 \
		delay --time $${MS:-500} $(EDGE_CONTAINER)

down:
	$(COMPOSE) --profile edge --profile chaos down

clean:
	$(COMPOSE) --profile edge --profile chaos down -v
