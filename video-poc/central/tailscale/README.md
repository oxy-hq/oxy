# Tailscale serve + funnel — bootstrap

`funnel-init.sh` is a one-shot sidecar script that runs on compose-up
to apply Tailscale Serve + Funnel against the live `tailscale` daemon.
Replaces the previous static `funnel.json` approach, which never
worked: tailscale doesn't expand `${TS_CERT_DOMAIN}` placeholders in
the JSON, so the serve rules never matched the real tailnet hostname.

## What it does

1. Waits up to 10 minutes for the daemon to report `BackendState=Running`.
   On a fresh boot with a valid `TS_AUTHKEY`, that's <30s.
2. Runs `tailscale serve --bg --https=443 http://mediamtx:8889` to
   proxy the public 443 endpoint at MediaMTX's WHEP signaling port.
3. Runs `tailscale funnel --bg 443` to publish that 443 endpoint to
   the public internet.
4. Reads the resolved hostname (`Self.DNSName` from `tailscale status
   --json`) and writes it to `/var/lib/outbox/funnel.env` as
   `EDGE_FUNNEL_HOSTNAME=<host>.<tailnet>.ts.net`. The edge worker's
   entrypoint sources that file on its next boot.

## Why the daemon socket is on a shared volume

The official `tailscale/tailscale` image runs `tailscaled` with its
CLI socket at `/var/run/tailscale/tailscaled.sock`. The funnel-init
sidecar shares the `tailscale-socket` named volume so its
`tailscale` CLI calls hit the same daemon — no need for `docker exec`
gymnastics or a `network_mode: service:tailscale` workaround.

## Failure modes

| Symptom | What probably happened |
|---|---|
| `funnel-init` exits 0 with "timed out waiting for tailscale login" | `TS_AUTHKEY` is unset or invalid in `/opt/oxy-edge/.env`. The rest of the pipeline still works; only live preview is dark. Set the key, restart `oxy-edge.service`. |
| `funnel-init` exits 1 with "serve apply failed" | Tailnet ACLs reject `tag:edge-box` from publishing on 443, or Funnel isn't enabled at the tailnet level. Fix in the Tailscale admin console. |
| Worker logs `edge.funnel_hostname_unset` | The `funnel.env` file wasn't there when the worker booted. Usually means the funnel-init sidecar is still warming up; the next worker restart picks it up. Or `TS_AUTHKEY` was missing. |
| Browser can't reach the WHEP URL | Verify `tailscale funnel status` shows 443 listed. Run `curl -v https://<your-hostname>.ts.net/` from outside the tailnet — should return MTX's HTTP banner. |

## Production note

The auth key is the only piece the operator has to provide. Pass it
via `install-edge.sh --ts-authkey tskey-auth-…` (the "Add device"
wizard's install snippet bakes this in). After that, the
funnel-init sidecar runs on every compose-up and live preview just
works without further hand-config. The hostname is auto-discovered;
the operator never needs to type it.

If you're running tailscale outside the compose stack (separate host
daemon, or a different network topology), set `EDGE_FUNNEL_HOSTNAME`
explicitly in `/opt/oxy-edge/.env` — it wins over the funnel-init's
auto-discovered value.

## Debugging

```bash
# Status from inside the daemon container
docker exec video-poc-tailscale tailscale status
docker exec video-poc-tailscale tailscale serve status
docker exec video-poc-tailscale tailscale funnel status

# Init-script log (what hostname did it write?)
docker logs video-poc-tailscale-funnel-init-1

# What the worker actually picked up
docker exec video-poc-edge-1 env | grep EDGE_FUNNEL_HOSTNAME
docker exec video-poc-edge-1 cat /var/lib/outbox/funnel.env
```
