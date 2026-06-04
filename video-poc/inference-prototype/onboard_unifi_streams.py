"""UniFi RTSPS stream URL fetcher (requires owner API key).

Companion to `onboard_unifi_inventory.py`. Once the customer has provided
an OWNER-level API key, this script fetches the actual streamable RTSPS
URL for every camera in their fleet via the connector proxy and emits
SQL UPDATEs to populate `cameras.rtsp_url`.

Endpoint used (one call per camera):
  GET https://api.ui.com/v1/connector/consoles/{console_id}
        /proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream

The cloud returns a ready-to-use URL that goes through Ubiquiti's relay,
so it works from anywhere — no port-forwarding required.

Until we have an owner key, this script still works as a probe: it tries
each camera, classifies the 403 vs other errors, and prints a clear
summary of which consoles need an owner key generated. Once any cameras
DO succeed, it emits the UPDATE SQL for those.

Usage:
  python onboard_unifi_streams.py                              # full fleet
  python onboard_unifi_streams.py --site Almaden         # filter
  python onboard_unifi_streams.py --out streams.sql            # write SQL
  python onboard_unifi_streams.py --inventory fleet.json       # reuse inventory
  python onboard_unifi_streams.py --dry-run                    # don't fetch URLs, just plan
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import sys
import time
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
    raise SystemExit("UNIFI_API_KEY not set. Add it to video-poc/.env or export it.")


def api_get(path: str, key: str, timeout: float = 30.0) -> tuple[int, bytes]:
    req = urllib.request.Request(
        API_BASE + path,
        headers={"X-API-KEY": key, "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.getcode(), r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()
    except Exception as e:
        return -1, str(e).encode()


def load_inventory(key: str, inventory_path: Path | None) -> dict[str, Any]:
    """Load inventory from a prior --json file, or pull fresh."""
    if inventory_path and inventory_path.exists():
        print(f"loading inventory from {inventory_path}", file=sys.stderr)
        return json.loads(inventory_path.read_text())
    print("pulling fresh inventory from api.ui.com ...", file=sys.stderr)
    # Mini-inventory: hostId + name + cameras list. Reuses the same endpoints
    # as the inventory script but doesn't bother with reachability probes.
    code, body = api_get("/v1/hosts", key)
    if code != 200:
        raise SystemExit(f"hosts list failed: {code} {body[:200]!r}")
    hosts = json.loads(body)["data"]
    code, body = api_get("/v1/devices", key)
    if code != 200:
        raise SystemExit(f"devices list failed: {code} {body[:200]!r}")
    by_host = {g["hostId"]: g for g in json.loads(body)["data"]}

    sites: list[dict[str, Any]] = []
    for h in hosts:
        g = by_host.get(h["id"], {})
        cams = [
            {
                "protect_camera_id": dv["id"],
                "name": dv.get("name", "?"),
                "model": dv.get("shortname"),
                "mac": dv.get("mac"),
                "online": dv.get("status") == "online",
            }
            for dv in g.get("devices", [])
            if dv.get("productLine") == "protect"
            and "Viewport" not in (dv.get("shortname") or "")
        ]
        sites.append({
            "host_id": h["id"],
            "name": g.get("hostName") or "?",
            "cameras": cams,
        })
    return {"sites": sites}


def fetch_rtsps(key: str, console_id: str, camera_id: str) -> tuple[str | int, dict[str, Any] | str]:
    """Returns (status_or_code, parsed_response_or_error_text)."""
    path = (
        f"/v1/connector/consoles/{console_id}/proxy/protect/integration/v1/"
        f"cameras/{camera_id}/rtsps-stream"
    )
    code, body = api_get(path, key, timeout=20.0)
    if code == 200:
        try:
            return 200, json.loads(body)
        except json.JSONDecodeError:
            return 200, body.decode(errors="replace")
    # Common non-200 cases we want to report clearly.
    msg = body.decode(errors="replace")[:200]
    try:
        # Many UniFi errors return JSON {code, message, ...}
        j = json.loads(body)
        msg = j.get("message") or msg
    except Exception:
        pass
    return code, msg


def extract_url(payload: dict[str, Any] | str) -> str | None:
    """Defensive: response shape isn't fully documented, try common fields."""
    if isinstance(payload, str):
        # Sometimes the server returns the URL as a raw string.
        if payload.startswith("rtsp"):
            return payload
        return None
    for key in ("url", "rtspsUrl", "rtspUrl", "streamUrl"):
        if key in payload and isinstance(payload[key], str):
            return payload[key]
    # Nested shape: {"data": {"url": "..."}}
    data = payload.get("data")
    if isinstance(data, dict):
        return extract_url(data)
    if isinstance(data, list) and data:
        return extract_url(data[0])
    return None


def sql_str(s: str | None) -> str:
    return "NULL" if s is None else "'" + s.replace("'", "''") + "'"


def deterministic_uuid(namespace: str, key: str) -> str:
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"{namespace}:{key}"))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--inventory", type=Path, help="reuse JSON from onboard_unifi_inventory.py --json")
    ap.add_argument("--out", type=Path, help="write SQL UPDATEs to file (default: stdout)")
    ap.add_argument("--site", help="filter to one site (substring, case-insensitive)")
    ap.add_argument("--dry-run", action="store_true", help="don't call /rtsps-stream; just print the plan")
    ap.add_argument("--max-concurrent", type=int, default=8)
    ap.add_argument("--sleep-ms", type=int, default=0, help="delay between calls (avoid rate limits)")
    args = ap.parse_args()

    key = load_api_key()
    inv = load_inventory(key, args.inventory)

    targets: list[tuple[dict[str, Any], dict[str, Any]]] = []
    for r in inv["sites"]:
        if args.site and args.site.lower() not in r["name"].lower():
            continue
        for c in r.get("cameras", []):
            targets.append((r, c))

    if args.dry_run:
        print(f"would call /rtsps-stream {len(targets)} times across "
              f"{len({r['host_id'] for r,_ in targets})} consoles", file=sys.stderr)
        for r, c in targets[:30]:
            print(f"  {r['name']:<28}  {c['name']:<28}  cam_id={c['protect_camera_id']}", file=sys.stderr)
        if len(targets) > 30:
            print(f"  ... and {len(targets) - 30} more", file=sys.stderr)
        return

    print(f"fetching /rtsps-stream for {len(targets)} cameras "
          f"(max_concurrent={args.max_concurrent})", file=sys.stderr)

    # Stats
    by_code: dict[int | str, int] = {}
    successes: list[tuple[dict[str, Any], dict[str, Any], str]] = []
    failures_by_host: dict[str, tuple[int | str, str]] = {}

    def worker(item: tuple[dict[str, Any], dict[str, Any]]) -> dict[str, Any]:
        r, c = item
        code, payload = fetch_rtsps(key, r["host_id"], c["protect_camera_id"])
        if args.sleep_ms:
            time.sleep(args.sleep_ms / 1000.0)
        return {"site": r, "camera": c, "code": code, "payload": payload}

    with cf.ThreadPoolExecutor(max_workers=args.max_concurrent) as ex:
        for res in ex.map(worker, targets):
            code = res["code"]
            by_code[code] = by_code.get(code, 0) + 1
            r, c = res["site"], res["camera"]
            if code == 200:
                url = extract_url(res["payload"])
                if url:
                    successes.append((r, c, url))
                else:
                    print(f"  200 but no url in payload: {r['name']}/{c['name']} -> "
                          f"{json.dumps(res['payload'])[:200]}", file=sys.stderr)
            else:
                # Record one per host (likely same error for all cameras on that host)
                if r["host_id"] not in failures_by_host:
                    failures_by_host[r["host_id"]] = (code, str(res["payload"])[:120])

    # Summary
    print("\n=== summary ===", file=sys.stderr)
    for code, n in sorted(by_code.items(), key=lambda x: -x[1]):
        print(f"  {code}  ×  {n}", file=sys.stderr)
    if failures_by_host:
        print("\nfailing hosts (one example per host):", file=sys.stderr)
        for r in inv["sites"]:
            if r["host_id"] in failures_by_host:
                code, msg = failures_by_host[r["host_id"]]
                print(f"  {r['name']:<28}  {code}  {msg}", file=sys.stderr)

    # Emit SQL UPDATEs for successes
    if successes:
        lines = [
            "-- Generated by onboard_unifi_streams.py — do not edit by hand.",
            "-- Updates rtsp_url on cameras already present (from onboard_unifi_inventory.py).",
            "",
            "BEGIN;",
        ]
        for r, c, url in successes:
            cam_uuid = deterministic_uuid("camera", c["protect_camera_id"] or c["mac"] or c["name"])
            lines.append(
                f"UPDATE cameras SET rtsp_url = {sql_str(url)} "
                f"WHERE id = {sql_str(cam_uuid)};"
            )
        lines.append("COMMIT;")
        sql = "\n".join(lines) + "\n"
        if args.out:
            args.out.write_text(sql)
            print(f"\n{len(successes)} stream URLs written to {args.out}", file=sys.stderr)
        else:
            print(sql)
    else:
        print("\nno stream URLs fetched. likely cause: no owner-level access to any "
              "of these consoles. Ask the customer to generate an owner API key.",
              file=sys.stderr)


if __name__ == "__main__":
    main()
