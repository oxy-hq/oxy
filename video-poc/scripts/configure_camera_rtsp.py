#!/usr/bin/env python3
"""Rewrite camera RTSP URLs from UniFi-local to public-NAT form.

## Problem this solves

UniFi Protect exposes its RTSP feeds on each site's local LAN as

    rtsps://<lan_ip>:7441/<alias>?enableSrtp

Those URLs aren't reachable from anywhere except the same LAN. For
testing the edge stack against real cameras without the worker
sitting on the restaurant's LAN, we need to point it at the UniFi
controller's public NAT instead:

    rtsp://<public_ip>:7447/<alias>

The transformation:

  * scheme       rtsps://      → rtsp://
  * host         <lan_ip>      → site.unifi_public_ip (from edge_boxes)
  * port         7441          → 7447  (UniFi NAT only opens 7447)
  * query        ?enableSrtp   → dropped (meaningless without SRTPS)
  * path         /<alias>      → unchanged

## Usage

The script takes a tiny CSV on stdin — one `name, local_url` pair
per line — and updates `cameras.rtsp_url` for each name that
matches a row in Oxy.

```
docker exec -i oxy-postgres psql -U oxy -d oxy <<<<no, like this:>>>
OXY_DATABASE_URL=postgresql://oxy:oxy@localhost:5432/oxy \\
  python3 video-poc/scripts/configure_camera_rtsp.py <<'EOF'
G4 Dome, rtsps://192.168.0.1:7441/eA7cpgei0WQ0lZl4?enableSrtp
G5 Bullet, rtsps://192.168.0.1:7441/abc123def?enableSrtp
EOF
```

The names must match `cameras.name` exactly (the value UniFi
import populates from the controller). Lines starting with `#` and
blank lines are ignored.

## Where this fits

This is the "I'm not at the restaurant but I want to point the dev
worker at a real camera" workflow. For dev against sim MP4 feeds,
seed cameras with rtsp_url pointing at the MTX sim paths directly
(no script needed).

## Workspace + multi-site

Defaults to the local-mode nil workspace. Cross-site name
collisions (two restaurants both have a "Kitchen Cam") are
flagged: the script refuses to update either, asks you to be
specific via --site-id.
"""
from __future__ import annotations

import argparse
import os
import sys
from urllib.parse import urlparse


def transform_url(local_url: str, public_ip: str, public_port: int = 7447) -> str:
    """Apply the rtsps→rtsp, host, port, query rewrites described
    above. Path (the stream alias) is preserved verbatim because that
    IS the camera's identifier on the controller.
    """
    parsed = urlparse(local_url)
    if not parsed.path:
        raise ValueError(f"local URL has no path component: {local_url!r}")
    # urlparse on rtsps:// preserves the path correctly; we just rebuild
    # with the new scheme/host/port and drop everything else.
    return f"rtsp://{public_ip}:{public_port}{parsed.path}"


def parse_pairs(stream) -> list[tuple[str, str]]:
    """Parse the comma-separated `name, local_url` format. Names with
    commas in them aren't supported — UniFi controller names don't
    typically contain commas, and the friction of a "smart parser"
    isn't worth it for a dev script.
    """
    pairs: list[tuple[str, str]] = []
    for lineno, raw in enumerate(stream, start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "," not in line:
            raise ValueError(f"line {lineno}: expected `name, url`, got {raw!r}")
        name, _, url = line.partition(",")
        pairs.append((name.strip(), url.strip()))
    return pairs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace-id",
        default=os.environ.get(
            "WORKSPACE_ID", "00000000-0000-0000-0000-000000000000"
        ),
        help="Workspace to update. Defaults to the local-mode nil UUID.",
    )
    parser.add_argument(
        "--site-id",
        help=(
            "Optional. If set, only cameras at this site are considered. "
            "Use when two sites in the workspace have cameras with the "
            "same name."
        ),
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would change without writing anything.",
    )
    parser.add_argument(
        "--public-port",
        type=int,
        default=7447,
        help="Public NAT port. Defaults to 7447 (UniFi convention).",
    )
    args = parser.parse_args()

    db_url = os.environ.get("OXY_DATABASE_URL")
    if not db_url:
        sys.exit(
            "OXY_DATABASE_URL not set. Typical value for local-mode dev:\n"
            "  postgresql://oxy:oxy@localhost:5432/oxy"
        )

    # psycopg is the only non-stdlib dep. Imported here so --help works
    # without it installed.
    try:
        import psycopg  # type: ignore[import-untyped]
    except ImportError:
        try:
            import psycopg2 as psycopg  # type: ignore[import-untyped,no-redef]
        except ImportError:
            sys.exit(
                "psycopg or psycopg2 is required.\n"
                "  pip install 'psycopg[binary]'   # newest, preferred\n"
                "or:\n"
                "  pip install psycopg2-binary"
            )

    pairs = parse_pairs(sys.stdin)
    if not pairs:
        sys.exit("no pairs on stdin — pass `name, url` lines via < or heredoc")

    conn = psycopg.connect(db_url)
    cur = conn.cursor()

    # Pre-resolve site → public_ip so we only join once per site, not
    # per camera. `edge_boxes.unifi_public_ip` is populated by the
    # UniFi import path (see crates/cameras/src/service/onboarding.rs).
    cur.execute(
        """
        SELECT s.id, s.name, COALESCE(eb.unifi_public_ip, NULL)
        FROM sites s
        LEFT JOIN edge_boxes eb
          ON eb.site_id = s.id AND eb.unifi_public_ip IS NOT NULL
        WHERE s.workspace_id = %s
        """,
        (args.workspace_id,),
    )
    sites = {row[0]: {"name": row[1], "public_ip": row[2]} for row in cur.fetchall()}
    if not sites:
        sys.exit(
            f"no sites in workspace {args.workspace_id}. "
            "Connect UniFi or seed sites first."
        )

    updated = 0
    skipped_no_match: list[str] = []
    skipped_no_public_ip: list[tuple[str, str]] = []
    skipped_ambiguous: list[tuple[str, list[str]]] = []

    for name, local_url in pairs:
        site_filter = (args.site_id,) if args.site_id else None
        params: tuple[str, ...] = (args.workspace_id, name)
        site_clause = ""
        if site_filter:
            site_clause = " AND c.site_id = %s"
            params = params + site_filter

        cur.execute(
            f"""
            SELECT c.id, c.site_id
            FROM cameras c
            JOIN sites s ON c.site_id = s.id
            WHERE s.workspace_id = %s AND c.name = %s{site_clause}
            """,
            params,
        )
        rows = cur.fetchall()
        if not rows:
            skipped_no_match.append(name)
            continue
        if len(rows) > 1 and not args.site_id:
            site_names = [sites[r[1]]["name"] for r in rows if r[1] in sites]
            skipped_ambiguous.append((name, site_names))
            continue

        cam_id, site_id = rows[0]
        public_ip = sites.get(site_id, {}).get("public_ip")
        if not public_ip:
            skipped_no_public_ip.append((name, sites.get(site_id, {}).get("name", "?")))
            continue

        new_url = transform_url(local_url, public_ip, args.public_port)
        site_label = sites[site_id]["name"]
        if args.dry_run:
            print(f"  [dry-run] {name} @ {site_label}  →  {new_url}")
        else:
            cur.execute(
                "UPDATE cameras SET rtsp_url = %s, updated_at = now() WHERE id = %s",
                (new_url, cam_id),
            )
            print(f"  ✓ {name} @ {site_label}  →  {new_url}")
        updated += 1

    if not args.dry_run:
        conn.commit()
    conn.close()

    print()
    print(
        f"{'Would update' if args.dry_run else 'Updated'} "
        f"{updated} camera{'s' if updated != 1 else ''}."
    )
    if skipped_no_match:
        print(f"⚠  No camera matched: {', '.join(skipped_no_match)}")
    if skipped_no_public_ip:
        for nm, site in skipped_no_public_ip:
            print(f"⚠  {nm} (site {site}): site has no unifi_public_ip set yet.")
    if skipped_ambiguous:
        for nm, sites_with_name in skipped_ambiguous:
            print(
                f"⚠  {nm}: ambiguous (in sites {sites_with_name}). "
                f"Re-run with --site-id."
            )
    return 0 if updated > 0 else 1


if __name__ == "__main__":
    sys.exit(main())
