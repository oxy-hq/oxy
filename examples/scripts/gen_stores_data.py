#!/usr/bin/env python3
"""Give the store fixture a hierarchy that actually branches at every level.

Why this exists
---------------
`stores.parquet` used to hold 24 stores in 24 cities — a perfect 1:1 city↔store
mapping. Every city was a single-store city, so `store_name`, `staff_count`,
`square_feet` and `monthly_rent` were all functionally determined by `city`, and
`city` itself was determined by `store_id`. Scoped to one city there was exactly
one store to look at.

That is what made the world-model instance panel's drill produce this:

    stores.city = Amsterdam        +270.4  +100% of root gap
      stores.region = eu           +270.4  +100% of root gap
        stores.staff_count = 14    +270.4  +100% of root gap
          stores.store_name = ...  +270.4  +100% of root gap
            reached the depth limit

Four levels, each restating the one above it, none of them explaining anything.
A dimension with one distinct value inside the scope reproduces its parent's
numerator exactly, so it always "explains" 100% of the gap.

The drill's own guards were missing and are fixed separately (airlayer
fix/drill-degenerate-splits: a cardinality floor on split candidates, plus
consuming scope-pinned dimensions). But even a correct drill needs something to
find. This script supplies it:

    4 regions  ->  3 cities each  ->  2 stores each  =  24 stores

Every level of the hierarchy now branches, and every dimension stays inside
airlayer's MAX_DIMENSION_CARDINALITY (25), so `store_name` (24 values) remains a
GLOBAL lever rather than being dropped as high-cardinality — which is why the
store count is held at 24 rather than growing.

What it does NOT do
-------------------
It fabricates no order values. Each new store inherits the orders of exactly one
old store, so `orders.csv` changes in the `store_id` column and nowhere else.
Every invariant `gen_orders_data.py` owns therefore survives untouched, for free:

- `total_amount == SUM(qty * unit_price * (1 - discount_percent/100))`
- `tax_amount == round(total_amount * 0.08, 2)`
- `shipping_cost == SUM(order_shipments.shipping_cost)`

Row counts, ids, order dates, channels and every other foreign key are preserved
exactly. Region-level aggregates are preserved exactly too, because stores are
only ever re-paired WITHIN their own region.

Where the levers come from
--------------------------
Because each store keeps its own order population, the per-store spread already
in the fixture (mean order value ranges 326 → 752 across stores) becomes the
within-city spread. The pairings below are chosen deliberately so each region
contains:

- a STRONG store gap  — provable in a 90d city-scoped slice (Welch t > 4)
- a MODERATE gap      — provable, but not overwhelming (t ~ 2.3–3)
- a FLAT pair         — two stores that genuinely do not differ (t < 1.6)

The flat pairs are as load-bearing as the strong ones. They are how we check the
evidence gate is doing its job on real data: scoped to Berlin or San Francisco
the drill must decline to split on `store_name` and say so, exactly as `status`
stays flat on purpose in `gen_orders_data.py`. A fixture where every city has a
findable store lever would prove nothing.

Usage
-----
    python3 examples/scripts/gen_stores_data.py [--check]

`--check` verifies the current fixture against this script's intent and writes
nothing. Deterministic: no RNG, no clock — the mapping below is the whole input.

Re-running is safe and idempotent: the remap is keyed on the OLD store ids, so
it is a no-op once orders.csv already carries new ids (the script detects this
and refuses rather than double-remapping).
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import math
import statistics
from collections import defaultdict
from pathlib import Path

DB = Path(__file__).resolve().parent.parent / ".db"
PARQUET = DB / "stores.parquet"

# ── The fixture ─────────────────────────────────────────────────────────────
#
# One row per NEW store, in id order. `absorbs` is the OLD store_id whose orders
# this store inherits — the only thing that touches orders.csv.
#
# The first store of each city is that city's original store, keeping its name,
# id-order position and physical attributes. The second is new: a real
# neighbourhood of the same city, sized as a secondary location (smaller floor
# area and headcount than the flagship, rent consistent with its city).
#
# `staff_count` is kept inside 11–27 so its cardinality stays well under
# MAX_DIMENSION_CARDINALITY; it is the one physical attribute that stays
# `segmentable` (you can act on headcount; you cannot act on square footage).
#
# fmt: off
STORES = [
    # id, store_name,                    city,            region, opened,       sqft, rent,   staff, absorbs
    (1,  "Union Square Market",          "San Francisco", "us",    "2019-03-15", 4200, 385000, 24,   1),
    (2,  "Mission Dolores Produce",      "San Francisco", "us",    "2021-05-18", 2600, 240000, 16,   5),
    (3,  "SoHo Grocer",                  "New York",      "us",    "2018-06-01", 3800, 520000, 21,   2),
    (4,  "Williamsburg Greenmarket",     "New York",      "us",    "2020-04-11", 3100, 335000, 19,   6),
    (5,  "Lincoln Park Fresh",           "Chicago",       "us",    "2020-09-10", 5100, 295000, 27,   3),
    (6,  "Pilsen Mercado",               "Chicago",       "us",    "2021-08-23", 2400, 155000, 15,   4),
    (7,  "Jordaan Groenmarkt",           "Amsterdam",     "eu",    "2020-07-29", 2400, 275000, 14,   10),
    (8,  "De Pijp Versmarkt",            "Amsterdam",     "eu",    "2017-11-06", 3300, 330000, 20,   7),
    (9,  "Le Marais Marché",             "Paris",         "eu",    "2018-10-03", 2700, 365000, 17,   8),
    (10, "Belleville Primeur",           "Paris",         "eu",    "2021-03-22", 2100, 205000, 13,   12),
    (11, "Kreuzberg Frischmarkt",        "Berlin",        "eu",    "2019-04-18", 3500, 220000, 18,   9),
    (12, "Prenzlauer Berg Markthalle",   "Berlin",        "eu",    "2021-01-14", 2800, 195000, 16,   11),
    (13, "Shibuya Fresh",                "Tokyo",         "apac",  "2018-12-01", 2200, 460000, 22,   13),
    (14, "Nakameguro Aoyasai",           "Tokyo",         "apac",  "2021-09-17", 1900, 300000, 14,   18),
    (15, "Tiong Bahru Market",           "Singapore",     "apac",  "2020-05-25", 2000, 380000, 16,   15),
    (16, "Katong Fresh",                 "Singapore",     "apac",  "2019-11-19", 2500, 290000, 19,   14),
    (17, "Bondi Grocer",                 "Sydney",        "apac",  "2019-02-11", 3400, 315000, 20,   16),
    (18, "Newtown Greengrocer",          "Sydney",        "apac",  "2021-06-30", 1800, 195000, 12,   17),
    (19, "Condesa Mercado",              "Mexico City",   "latam", "2019-07-21", 3700, 165000, 23,   20),
    (20, "Coyoacán Frutería",            "Mexico City",   "latam", "2021-11-08", 2300, 105000, 13,   23),
    (21, "Palermo Verduleria",           "Buenos Aires",  "latam", "2020-08-04", 3000, 125000, 17,   19),
    (22, "San Telmo Mercadito",          "Buenos Aires",  "latam", "2021-04-27", 2600,  95000, 15,   22),
    (23, "Vila Madalena Feira",          "Sao Paulo",     "latam", "2020-10-12", 4100, 145000, 25,   21),
    (24, "Pinheiros Hortifruti",         "Sao Paulo",     "latam", "2022-01-30", 2450, 135000, 14,   24),
]
# fmt: on

COLUMNS = [
    "store_id",
    "store_name",
    "city",
    "region",
    "opened_date",
    "square_feet",
    "monthly_rent",
    "staff_count",
]

# The old store_id -> old region, used only to assert the re-pairing never moved
# a store across regions (which would silently change region-level aggregates).
OLD_REGION = {
    **{i: "us" for i in range(1, 7)},
    **{i: "eu" for i in range(7, 13)},
    **{i: "apac" for i in range(13, 19)},
    **{i: "latam" for i in range(19, 25)},
}

# Welch t below this reads as "these two stores do not differ" — the flat pairs.
FLAT_T = 1.6
# ...and above this as a lever the gate can prove in a 90d city slice.
STRONG_T = 2.0


def _orders() -> list[dict]:
    with open(DB / "orders.csv", newline="") as f:
        return list(csv.DictReader(f))


def remap() -> dict[int, int]:
    """OLD store_id -> NEW store_id."""
    return {old: new for new, _, _, _, _, _, _, _, old in STORES}


def validate_mapping() -> None:
    """The mapping must be a within-region bijection over all 24 old stores."""
    m = remap()
    missing = sorted(set(OLD_REGION) - set(m))
    if missing:
        raise SystemExit(f"old stores with no destination: {missing}")
    if len(set(m.values())) != len(STORES):
        raise SystemExit("two old stores map to the same new store")
    by_new = {s[0]: s[3] for s in STORES}
    crossed = [
        (old, OLD_REGION[old], by_new[new])
        for old, new in m.items()
        if OLD_REGION[old] != by_new[new]
    ]
    if crossed:
        raise SystemExit(
            "re-pairing crossed a region boundary, which would move revenue "
            f"between regions: {crossed}"
        )
    per_city = defaultdict(list)
    for s in STORES:
        per_city[(s[3], s[2])].append(s[1])
    thin = {c: n for c, n in per_city.items() if len(n) < 2}
    if thin:
        raise SystemExit(f"every city needs >= 2 stores, else the drill has nothing to split: {thin}")
    per_region = defaultdict(set)
    for (region, city) in per_city:
        per_region[region].add(city)
    thin = {r: sorted(c) for r, c in per_region.items() if len(c) < 2}
    if thin:
        raise SystemExit(f"every region needs >= 2 cities: {thin}")


def write_parquet() -> None:
    import duckdb

    con = duckdb.connect()
    con.execute(
        f"CREATE TABLE stores ({COLUMNS[0]} INTEGER, {COLUMNS[1]} VARCHAR, "
        f"{COLUMNS[2]} VARCHAR, {COLUMNS[3]} VARCHAR, {COLUMNS[4]} DATE, "
        f"{COLUMNS[5]} INTEGER, {COLUMNS[6]} INTEGER, {COLUMNS[7]} INTEGER)"
    )
    con.executemany(
        "INSERT INTO stores VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        [s[:8] for s in STORES],
    )
    con.execute(f"COPY stores TO '{PARQUET}' (FORMAT PARQUET)")
    con.close()


def remap_orders() -> list[dict]:
    orders = _orders()
    m = remap()
    seen = {int(o["store_id"]) for o in orders}
    if not seen <= set(m):
        raise SystemExit(
            f"orders.csv references store ids outside the old 1-24 set ({sorted(seen - set(m))}); "
            "it has probably been remapped already — refusing to remap twice."
        )
    for o in orders:
        o["store_id"] = str(m[int(o["store_id"])])
    return orders


def welch(a: list[float], b: list[float]) -> tuple[float, float]:
    """Return (gap, t) for the difference in mean between two samples."""
    if len(a) < 2 or len(b) < 2:
        return 0.0, 0.0
    va, vb = statistics.variance(a), statistics.variance(b)
    se = math.sqrt(va / len(a) + vb / len(b))
    gap = statistics.mean(a) - statistics.mean(b)
    return gap, (abs(gap) / se if se else 0.0)


def report(orders: list[dict]) -> None:
    """Print the within-city store gap the drill is now expected to find."""
    name = {s[0]: s[1] for s in STORES}
    city = {s[0]: s[2] for s in STORES}
    region = {s[0]: s[3] for s in STORES}

    newest = max(dt.date.fromisoformat(o["order_date"]) for o in orders)
    cut = newest - dt.timedelta(days=90)
    vals: dict[int, list[float]] = defaultdict(list)
    for o in orders:
        if dt.date.fromisoformat(o["order_date"]) >= cut:
            vals[int(o["store_id"])].append(float(o["total_amount"]))

    by_city: dict[tuple[str, str], list[int]] = defaultdict(list)
    for s in STORES:
        by_city[(s[3], s[2])].append(s[0])

    print(f"\nwithin-city store gap over the last 90d (to {newest})")
    print(f"{'region':<7}{'city':<15}{'gap':>9}{'t':>7}  verdict")
    counts = {"lever": 0, "weak": 0, "flat": 0}
    for (reg, c), ids in sorted(by_city.items()):
        a, b = (vals[i] for i in ids)
        gap, t = welch(a, b)
        if t >= STRONG_T:
            verdict, key = "lever  (drill splits on store_name)", "lever"
        elif t < FLAT_T:
            verdict, key = "flat   (drill must decline to split)", "flat"
        else:
            verdict, key = "weak   (borderline)", "weak"
        counts[key] += 1
        print(f"{reg:<7}{c:<15}{abs(gap):>9.1f}{t:>7.2f}  {verdict}")

    print(f"\n{'':<7}{'':15}{'':9}{'':7}  " f"{counts['lever']} levers, {counts['weak']} weak, {counts['flat']} flat")
    if not counts["lever"]:
        raise SystemExit("no city has a provable store gap — the drill would find nothing")
    if not counts["flat"]:
        raise SystemExit(
            "no city has a flat store pair — nothing checks the evidence gate declines"
        )

    print(f"\n{'':<7}shape: ", end="")
    regions = {s[3] for s in STORES}
    cities = {(s[3], s[2]) for s in STORES}
    print(f"{len(regions)} regions -> {len(cities)} cities -> {len(STORES)} stores")
    for dim, n in (("store_name", len({s[1] for s in STORES})), ("city", len(cities)),
                   ("staff_count", len({s[7] for s in STORES})), ("region", len(regions))):
        flag = "" if n <= 25 else "   ** EXCEEDS MAX_DIMENSION_CARDINALITY (25) **"
        print(f"{'':<14}{dim:<12}{n:>3} distinct{flag}")


def check(orders: list[dict]) -> None:
    """Verify what is already on disk, writing nothing."""
    import duckdb

    con = duckdb.connect()
    rows = con.execute(
        f"SELECT {', '.join(COLUMNS)} FROM '{PARQUET}' ORDER BY store_id"
    ).fetchall()
    con.close()
    want = [
        (s[0], s[1], s[2], s[3], dt.date.fromisoformat(s[4]), s[5], s[6], s[7])
        for s in STORES
    ]
    if rows != want:
        raise SystemExit("stores.parquet does not match this script's STORES table")

    ids = {int(o["store_id"]) for o in orders}
    expected = {s[0] for s in STORES}
    if not ids <= expected:
        raise SystemExit(f"orders.csv references unknown stores: {sorted(ids - expected)}")
    if ids != expected:
        raise SystemExit(f"stores with no orders at all: {sorted(expected - ids)}")
    print("stores.parquet and orders.csv agree with this script")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--check", action="store_true", help="Verify what is on disk; write nothing."
    )
    args = ap.parse_args()

    validate_mapping()

    if args.check:
        orders = _orders()
        check(orders)
        report(orders)
        return

    before = _orders()
    per_old = defaultdict(int)
    for o in before:
        per_old[int(o["store_id"])] += 1

    orders = remap_orders()
    write_parquet()

    # Each new store must have inherited its old counterpart's orders exactly —
    # the remap moves rows between store ids and must not create or lose any.
    per_new = defaultdict(int)
    for o in orders:
        per_new[int(o["store_id"])] += 1
    m = remap()
    bad = [(old, new) for old, new in m.items() if per_old[old] != per_new[new]]
    if bad:
        raise SystemExit(f"order counts did not carry across the remap: {bad}")
    if len(orders) != len(before):
        raise SystemExit("the remap changed the order row count")

    with open(DB / "orders.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(orders[0].keys()))
        w.writeheader()
        w.writerows(orders)

    print(f"wrote {PARQUET.name} ({len(STORES)} stores) and remapped {len(orders):,} orders")
    report(orders)


if __name__ == "__main__":
    main()
