"""UniFi fleet inventory + SQL generator.

Pulls sites and cameras from the UniFi Site Manager cloud API
(api.ui.com), probes which public IPs have RTSP open externally, and
emits ready-to-paste SQL INSERTs for our sites / edge_boxes /
cameras tables.

Auth: needs `UNIFI_API_KEY` in env or in video-poc/.env. Admin-level
permission on the consoles is sufficient — owner is NOT required for
inventory enumeration.

What this gets you WITHOUT owner permission:
  - Full site list (host names + public IPs + console IDs)
  - Full camera list per site (Protect ObjectId, name, model,
    private IP, MAC, online/offline status, firmware)
  - TCP probe per site for RTSP-port reachability (7447, 7441)

What you still need owner permission for:
  - Per-camera rtspAlias (the path component in the actual RTSP URL)
  - /rtsps-stream endpoint that hands back a ready-to-use URL
  - Toggling channel.isRtspEnabled programmatically

If a site has port 7447 open AND the customer has manually enabled
RTSP per camera, you can still reach the stream with the alias they share.

Usage:
  python onboard_unifi_inventory.py                          # print summary + SQL to stdout
  python onboard_unifi_inventory.py --out inventory.sql      # write SQL to file
  python onboard_unifi_inventory.py --json fleet.json        # also write JSON inventory
  python onboard_unifi_inventory.py --site Almaden     # filter to one host
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import socket
import sys
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any

API_BASE = "https://api.ui.com"
ENV_FILE = Path(__file__).resolve().parent.parent / ".env"


def load_api_key() -> str:
    key = os.environ.get("UNIFI_API_KEY")
    if key:
        return key
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("UNIFI_API_KEY="):
                return line.split("=", 1)[1].strip()
    raise SystemExit(
        "UNIFI_API_KEY not set. Add it to video-poc/.env or export it in your shell."
    )


def api_get(path: str, key: str) -> dict[str, Any]:
    req = urllib.request.Request(
        API_BASE + path,
        headers={"X-API-KEY": key, "Accept": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


def list_hosts(key: str) -> list[dict[str, Any]]:
    return api_get("/v1/hosts", key)["data"]


def list_devices(key: str) -> list[dict[str, Any]]:
    return api_get("/v1/devices", key)["data"]


def host_detail(host_id: str, key: str) -> dict[str, Any]:
    return api_get(f"/v1/hosts/{host_id}", key)["data"]


def probe_tcp(ip: str | None, port: int, timeout: float = 3.0) -> bool:
    if not ip:
        return False
    try:
        with socket.create_connection((ip, port), timeout=timeout):
            return True
    except OSError:
        return False


def sql_str(s: str | None) -> str:
    if s is None:
        return "NULL"
    return "'" + s.replace("'", "''") + "'"


def sql_bool(b: bool | None) -> str:
    return "NULL" if b is None else ("TRUE" if b else "FALSE")


def deterministic_uuid(namespace: str, key: str) -> str:
    """Produce a stable UUID so re-running the script doesn't create duplicate rows."""
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"{namespace}:{key}"))


def build_inventory(key: str, site_filter: str | None = None) -> dict[str, Any]:
    print("fetching /v1/hosts ...", file=sys.stderr)
    hosts = list_hosts(key)
    print(f"  {len(hosts)} host(s)", file=sys.stderr)

    print("fetching /v1/devices ...", file=sys.stderr)
    devs_by_host = {g["hostId"]: g for g in list_devices(key)}

    # Per-host detail fetched in parallel — we want the public IP and reportedState.name.
    def fetch_one(h: dict[str, Any]) -> dict[str, Any]:
        try:
            d = host_detail(h["id"], key)
        except urllib.error.HTTPError as e:
            return {"host_id": h["id"], "error": f"{e.code} {e.reason}"}
        rs = d.get("reportedState", {})
        return {
            "host_id": h["id"],
            "hardware_id": h.get("hardwareId"),
            "name": rs.get("name") or h.get("hostname") or "?",
            "hostname": rs.get("hostname"),
            "public_ip": h.get("ipAddress") or d.get("ipAddress"),
            "hardware_model": rs.get("hardware", {}).get("shortname"),
            "timezone": rs.get("timezone"),
            "release_channel": rs.get("releaseChannel"),
        }

    print("fetching per-host details in parallel ...", file=sys.stderr)
    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        details = list(ex.map(fetch_one, hosts))

    inventory: list[dict[str, Any]] = []
    for det in details:
        if det.get("error"):
            print(f"  warn: {det['host_id'][:18]}…  {det['error']}", file=sys.stderr)
            continue
        if site_filter and site_filter.lower() not in det["name"].lower():
            continue
        # Match devices for this host.
        dev_group = devs_by_host.get(det["host_id"], {})
        cameras = []
        for dv in dev_group.get("devices", []):
            if dv.get("productLine") != "protect":
                continue
            if "Viewport" in (dv.get("shortname") or ""):
                continue  # Display devices, not cameras
            cameras.append({
                "protect_camera_id": dv.get("id"),
                "name": dv.get("name") or "?",
                "model": dv.get("shortname") or dv.get("model"),
                "mac": dv.get("mac"),
                "private_ip": dv.get("ip"),
                "online": (dv.get("status") == "online"),
                "version": dv.get("version"),
            })
        det["cameras"] = cameras
        inventory.append(det)

    # Probe RTSP port reachability for each site in parallel.
    print("probing port 7447 (RTSP) and 7441 (RTSPS) across the fleet ...", file=sys.stderr)

    def probe(d: dict[str, Any]) -> dict[str, Any]:
        d["rtsp_7447_open"] = probe_tcp(d["public_ip"], 7447)
        d["rtsps_7441_open"] = probe_tcp(d["public_ip"], 7441)
        return d

    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        inventory = list(ex.map(probe, inventory))

    return {"sites": inventory}


def print_summary(inv: dict[str, Any]) -> None:
    rs = inv["sites"]
    total_cams = sum(len(r["cameras"]) for r in rs)
    online_cams = sum(1 for r in rs for c in r["cameras"] if c["online"])
    open_sites = sum(1 for r in rs if r["rtsp_7447_open"])
    print(f"\n{'Site':<28}  {'Public IP':<16}  {'Cameras':>7}  {'Online':>6}  {'RTSP':>6}  {'RTSPS':>6}",
          file=sys.stderr)
    print("-" * 80, file=sys.stderr)
    for r in sorted(rs, key=lambda x: x["name"]):
        ncams = len(r["cameras"])
        non = sum(1 for c in r["cameras"] if c["online"])
        rtsp = "OPEN" if r["rtsp_7447_open"] else "—"
        rtsps = "OPEN" if r["rtsps_7441_open"] else "—"
        print(f"  {r['name']:<26}  {str(r['public_ip']):<16}  {ncams:>7}  {non:>6}  {rtsp:>6}  {rtsps:>6}",
              file=sys.stderr)
    print(f"\n  TOTAL: {len(rs)} sites, {total_cams} cameras ({online_cams} online), "
          f"{open_sites} with public RTSP", file=sys.stderr)


def emit_sql(inv: dict[str, Any]) -> str:
    """Emit idempotent SQL INSERTs for sites, edge_boxes, cameras.

    Uses uuid5 (namespace=URL) on stable identifiers so re-runs are idempotent
    via ON CONFLICT DO NOTHING / DO UPDATE. The deterministic UUIDs are tied
    to UniFi's console_id, host's mac_address (for cameras), and don't change
    if the customer renames things in Protect.
    """
    lines = [
        "-- Generated by onboard_unifi_inventory.py — do not edit by hand.",
        "-- Idempotent: safe to re-run; existing rows are updated.",
        "",
        "BEGIN;",
        "",
    ]
    for r in inv["sites"]:
        rest_uuid = deterministic_uuid("site", r["host_id"])
        box_uuid = deterministic_uuid("edge_box", r["host_id"])
        lines.append(f"-- {r['name']}  ({len(r['cameras'])} cameras, public_ip={r['public_ip']})")
        lines.append(
            f"INSERT INTO sites (id, name, timezone, region) VALUES "
            f"({sql_str(rest_uuid)}, {sql_str(r['name'])}, "
            f"{sql_str(r.get('timezone') or 'UTC')}, NULL) "
            f"ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, timezone = EXCLUDED.timezone;"
        )
        lines.append(
            f"INSERT INTO edge_boxes (id, site_id, hardware_model, image_tag, status, "
            f"unifi_console_id, unifi_public_ip, unifi_rtsp_reachable) VALUES "
            f"({sql_str(box_uuid)}, {sql_str(rest_uuid)}, "
            f"{sql_str('unifi-controller')}, {sql_str('cloud')}, {sql_str('active')}, "
            f"{sql_str(r['host_id'])}, {sql_str(r['public_ip'])}, "
            f"{sql_bool(r['rtsp_7447_open'])}) "
            f"ON CONFLICT (id) DO UPDATE SET "
            f"unifi_public_ip = EXCLUDED.unifi_public_ip, "
            f"unifi_rtsp_reachable = EXCLUDED.unifi_rtsp_reachable;"
        )
        for c in r["cameras"]:
            cam_uuid = deterministic_uuid("camera", c["protect_camera_id"] or c["mac"] or c["name"])
            # rtsp_url left empty until rtspAlias is known (owner-key step).
            lines.append(
                f"INSERT INTO cameras (id, site_id, edge_box_id, name, rtsp_url, "
                f"analytics_consent, active, protect_camera_id, mac_address, model, online) VALUES "
                f"({sql_str(cam_uuid)}, {sql_str(rest_uuid)}, {sql_str(box_uuid)}, "
                f"{sql_str(c['name'])}, {sql_str('')}, "      # rtsp_url filled in once rtspAlias is known
                f"FALSE, TRUE, "                              # consent default OFF — flip per-camera as needed
                f"{sql_str(c['protect_camera_id'])}, {sql_str(c['mac'])}, "
                f"{sql_str(c['model'])}, {sql_bool(c['online'])}) "
                f"ON CONFLICT (site_id, name) DO UPDATE SET "
                f"protect_camera_id = EXCLUDED.protect_camera_id, "
                f"mac_address = EXCLUDED.mac_address, model = EXCLUDED.model, "
                f"online = EXCLUDED.online;"
            )
        lines.append("")
    lines.append("COMMIT;")
    return "\n".join(lines) + "\n"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, help="write SQL to this file (default: stdout)")
    ap.add_argument("--json", dest="json_out", type=Path, help="also write JSON inventory")
    ap.add_argument("--site", help="filter to one site (case-insensitive substring match)")
    args = ap.parse_args()

    key = load_api_key()
    inv = build_inventory(key, site_filter=args.site)
    print_summary(inv)

    sql = emit_sql(inv)
    if args.out:
        args.out.write_text(sql)
        print(f"\nSQL written to {args.out} ({len(sql.splitlines())} lines)", file=sys.stderr)
    else:
        print(sql)

    if args.json_out:
        args.json_out.write_text(json.dumps(inv, indent=2, default=str))
        print(f"JSON written to {args.json_out}", file=sys.stderr)


if __name__ == "__main__":
    main()
