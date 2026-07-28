#!/usr/bin/env python3
"""Generate the restaurant demo dataset: a 24-location chain shaped so five
independent opportunity signals, a three-level revenue/profit tree, and two
deliberate nulls all coexist on the same 104,000-check fixture.

Why this exists
---------------
The opportunity drill decomposes a metric against a benchmark and reports
which branch, and which dimension, owns the gap. That is only a meaningful
demo if several things are true at once: more than one dimension can
plausibly explain the same headline number (so the drill has to pick a
winner, not restate the only story available); the decomposition runs more
than one level deep (so a drill that stops after one level and a drill that
recurses give different answers); ranking by revenue and ranking by profit
can disagree (so a second measure at the leaf is worth reading, not
decoration); and some of what is planted is a null (so a significance gate
that calls everything "significant" is indistinguishable from one that
filters). This fixture is built to make all four true at once, and
`--check` (see `check()`) re-measures every number in this docstring from
whatever is currently on disk — re-run it after any generator change and
update the figures below from its printed output, not from memory.

The five signals, and why they are independent
------------------------------------------------
Five opportunity axes are planted on the same 104,000 checks, each thinning
the baseline add-on attach rate multiplicatively (`ATTACH_MODIFIERS`) on top
of whatever the other axes already did to that check:

    signal                  axis      add-on gap/check   n (segment)
    midwest                 region          13.715           22,249
    delivery                channel         17.984           15,519
    new_server (<3mo)       tenure           4.957           20,930
    late_night              daypart          3.421           10,411
    broken_location (#20)   location        10.236            3,304

(measured by `--check` on 2026-07-20; `_addon_revenue_gap` is the shared
measurement helper Task 5's gap-floor assertions read from)

They overlap multiplicatively on the same checks — a check can be midwest
AND delivery AND rung by a new server AND late night, all four discounts
compounding — but each is drawn from its own independent weighted choice
(region implicitly via location, channel, tenure band, daypart), none
conditioned on any other. That independence is measured, not assumed:
channel, tenure-band, and daypart shares each sit within about 1.4
percentage points of each other across all four regions (the tenure check is
asserted directly in `check()`, at a looser 4-point tolerance). That is what
makes the midwest headline genuinely a regional attach problem rather than a
channel-mix or staffing skew wearing a region's name — if delivery or
new-server share leaned toward midwest, "the midwest gap" could really just
be "the delivery gap," relabeled.

Upside is gap x volume (`_addon_revenue_gap` again), so both terms matter to
ranking: `check()` asserts midwest and delivery are the top two opportunities
by that product. The other three signals exist to prove the drill can also
surface a real but smaller opportunity, not just the two loudest ones.

The three-level component tree
---------------------------------
    net_revenue       = entree_revenue + addon_revenue
    addon_revenue     = sides_revenue + beverages_revenue
                        + appetizers_revenue + desserts_revenue
    beverages_revenue = alcoholic_revenue + na_beverage_revenue

Every level is an exact additive identity — `check()` verifies all three
independently of the flat `total_amount` check — and every revenue measure
has a `*_profit` twin, so the same tree walks on either measure. Ranked on
revenue the largest add-on gap is sides ($7.045/check); ranked on profit it's
beverages ($3.744/check vs sides' $3.180), because sides carry a 55% food
cost and fountain drinks carry 12% — retained margin inverts the ranking.
See `checks.view.yml` for the SQL this tree is realized in.

The third level exists because delivery orders carry exactly zero alcoholic
revenue (`check()` asserts `chan_alc["delivery"] == 0.0`) — a courier cannot
carry a cocktail. That is what lets a drill on the delivery signal descend
through all three levels — `net_revenue` -> `addon_revenue` ->
`beverages_revenue` -> `alcoholic_revenue` — and land on "we sell no alcohol
on delivery" instead of stopping one level up at the vaguer "beverages are
down." A tree that bottomed out at `beverages_revenue` could not tell that
more specific story.

The two deliberate nulls
--------------------------
- **desserts** is null on three of the five segmenting axes, not all of
  them: region, daypart, and location_name (broken-location gap +0.035,
  z +0.22). Measured region-gap z = -0.12 (revenue),
  -0.18 (profit); daypart gap -0.023/check, z ~ -0.2 (`_category_gap_z`,
  which is hardcoded to the region axis — see its docstring). On the remaining
  two axes desserts is *deliberately thinned*, not null: `ATTACH_MODIFIERS`
  cuts it to 0.70x on delivery and 0.58x for under-3mo servers, and that
  shows up as a real, large gap — +1.386/check (z +19.3) on delivery,
  +2.078/check (z +33.7) on under_3mo. That is intentional (delivery orders
  and green servers plausibly do sell fewer desserts) but it means "desserts
  is the null" is only true on three of the fixture's five axes. In
  particular, the verification script's only drill root
  (`order_channel = delivery`) lands exactly on an axis where desserts is a
  19-SE signal, not a null — a maintainer who reads "desserts is the
  component-level null" and expects it to stay quiet under that drill will
  be surprised. **table_section** (main/patio/bar) is the fixture's only
  *globally* inert dimension — no relationship to any measure, on any axis;
  it is drawn independently of everything, revenue included. Measured
  spread across sections: ~$0.34/check, against a ~$142/check average.

A fixture where every dimension the engine looks at comes back significant
cannot tell a working significance gate from one that rubber-stamps whatever
it is handed. These two exist so the gate can be shown actually dropping
them (into `segments_dropped_as_noise`, in the engine — not in this file)
rather than reporting a false lead.

Why 24 locations
-------------------
The engine treats any dimension with 25 or more distinct values as a
high-cardinality identifier and skips it as an opportunity axis. 24
locations keeps `location_name` one value below that ceiling; 25 would
silently delete it as a drill axis with no error anywhere. `check()` asserts
the ceiling directly (`CARDINALITY_CEILING = 25`) against every dimension
this fixture segments on — location_name 24, region 4, city 17,
order_channel 5, tenure_band 4, table_section 3 — rather than trusting
`len(LOCATIONS)`, so a future edit that grows the fleet, or any other axis,
fails loudly instead of quietly losing an axis.

Guard statistics, in standard errors, not dollars
----------------------------------------------------
Every guard in `check()` that actually protects this fixture is expressed as
a z-score — a multiple of the measured standard error — rather than a dollar
floor, because a dollar threshold silently decalibrates whenever sample size
or price level change, and both have, here, in this fixture's own history.
The desserts null used to be gated on a flat $0.05 threshold; since then the
fixture grew from ~45k checks to 104k (which shrinks SE) and menu prices rose
~1.85x (which grows both the gap and the SE), and the measured null gap
drifted to 0.70 SE / 1.12 SE of ordinary coin-flip noise — roughly a coin
flip — while the fixed $0.05 number never moved, decalibrating the gate with
no code change to flag it. A z-score self-scales with whatever sample size
and prices the fixture happens to have on a given run:

- `MIN_MARGIN_Z = 4.0` — the sides-vs-beverages revenue margin and the
  beverages-vs-sides profit margin must each clear 4 SE. Measured: revenue
  margin z = 15.71 (diff $2.492/check, SE $0.159); profit margin z = 5.01
  (diff $0.565/check, SE $0.113). This is the guard that actually protects
  the demo — the ranking checks below it (`top_rev == "sides"`) pass just as
  happily at 1 SE as at 30 SE, which is exactly how this margin eroded to
  ~1 SE unnoticed when cocktails were added in 80a2d7dcc.
- `NULL_MAX_Z = 3.0` — desserts must stay under 3 SE on both revenue and
  profit. Measured z = -0.12 / -0.18.
- `TABLE_SECTION_MAX_Z = 3.0` — the two most divergent table_section means
  (main vs patio) must stay under 3 SE, via the same `_bench_seg_z` tail
  used by the guards above. Measured z = 0.61 (diff $0.338/check,
  SE $0.557/check).

The size budget
------------------
Committed CSV is budgeted at ~15 MB; the five files on disk currently total
13,859,971 bytes (~13.9 MB). `restaurant_checks.csv` and
`restaurant_check_items.csv` (446,726 line items for 104,000 checks) are the
two files that scale with check volume, so that headroom — not aesthetics —
is why `N_CHECKS` stops at 104,000: a further increase without shrinking
something else would blow the budget.

Invariants
----------
These hold at 100% and other measures are built on them, so downstream views
derive rather than restate:

- `checks.total_amount == SUM(quantity * unit_price)` over the check's items
- `checks.tax_amount == round(total_amount * 0.0875, 2)`
- per-line cost is `quantity * menu_items.unit_cost` (check_items carries no
  `extended_cost` column of its own); unit_cost is a fixed per-menu-item value,
  never re-drawn per line — so gross margin by category is a property of the
  menu, not of sampling noise
- `checks.total_amount == entree_revenue + sides + beverages + appetizers +
  desserts`, summed per check from the line items joined to the menu — the
  first two levels of the tree `checks.view.yml` is built on
- `beverages_revenue == alcoholic_revenue + na_beverage_revenue` per check —
  the third level, and the one delivery's zero-alcohol signal depends on
- tenure_band is independent of region (within a 4-point share tolerance per
  band), and order_channel/table_section are drawn independently of every
  other axis, so each of the five signals can be asserted in isolation
- every `check_items.check_id` resolves to a check; every `menu_item_id`
  resolves to a menu item; every `checks.server_id` resolves to a server

Usage
-----
    python3 example_new/scripts/gen_restaurant_data.py [--anchor YYYY-MM-DD] [--check]

`--check` verifies the invariants, the revenue/profit inversion, the five
signal gaps, and the two nulls against what is already on disk, and writes
nothing. Deterministic: same seed and anchor in, same bytes out.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import math
import random
from collections import defaultdict
from pathlib import Path

DB = Path(__file__).resolve().parent.parent / ".db"
SEED = 20260720
ANCHOR = "2026-07-19"
N_CHECKS = 104_000
DAYS = 400
TAX_RATE = 0.0875

# ── Locations ───────────────────────────────────────────────────────────────
#
# 4 regions, 24 locations. midwest is the underperforming segment and is
# deliberately a minority of the fleet (5/24) — a segment that is most of the
# chain would make the "benchmark" mostly itself and shrink the measurable gap.
LOCATIONS = [
    (1, "Downtown Loop", "midwest", "Chicago"),
    (2, "River North", "midwest", "Chicago"),
    (3, "Wicker Park", "midwest", "Chicago"),
    (4, "Cherry Creek", "midwest", "Denver"),
    (5, "Country Club Plaza", "midwest", "Kansas City"),
    (6, "Mission District", "west", "San Francisco"),
    (7, "Hayes Valley", "west", "San Francisco"),
    (8, "Santa Monica", "west", "Los Angeles"),
    (9, "Silver Lake", "west", "Los Angeles"),
    (10, "Pearl District", "west", "Portland"),
    (11, "Capitol Hill", "west", "Seattle"),
    (12, "Scottsdale Quarter", "west", "Phoenix"),
    (13, "Back Bay", "northeast", "Boston"),
    (14, "Seaport", "northeast", "Boston"),
    (15, "West Village", "northeast", "New York"),
    (16, "Brooklyn Heights", "northeast", "New York"),
    (17, "Tribeca", "northeast", "New York"),
    (18, "Rittenhouse Square", "northeast", "Philadelphia"),
    (19, "Georgetown", "northeast", "Washington"),
    (20, "Buckhead", "south", "Atlanta"),
    (21, "South Congress", "south", "Austin"),
    (22, "Deep Ellum", "south", "Dallas"),
    (23, "Coconut Grove", "south", "Miami"),
    (24, "Music Row", "south", "Nashville"),
]
SEGMENT_REGION = "midwest"
# The single underperforming location, in `south` on purpose: inside `midwest`
# it would be indistinguishable from the regional attach deficit.
BROKEN_LOCATION_ID = 20

# ── Menu ────────────────────────────────────────────────────────────────────
#
# (id, name, category, item_type, price, cost_ratio)
#
# cost_ratio is food cost as a fraction of menu price — the lever that makes
# revenue and profit rank differently:
#
#   beverages 0.12  fountain soda and iced tea; pennies of syrup on a $4 pour
#   entrees   0.32  industry-standard plate cost
#   desserts  0.38  bought-in, moderate cost
#   appetizers 0.40 shared plates, generous portions
#   sides     0.55  the thin one — big cheap-looking portions on a low ticket
#
# The sides/beverages spread (0.45 vs 0.88 retained margin) is what inverts the
# ranking. It is not exaggerated: it is roughly the real spread between a fryer
# side and a soda gun in casual dining.
MENU = [
    # entrees
    (101, "Grilled Ribeye", "entrees", "entree", 62.00, 0.32, ""),
    (102, "Roast Chicken", "entrees", "entree", 41.00, 0.32, ""),
    (103, "Pan-Seared Salmon", "entrees", "entree", 49.00, 0.33, ""),
    (104, "Short Rib Pappardelle", "entrees", "entree", 44.00, 0.31, ""),
    (105, "Mushroom Risotto", "entrees", "entree", 36.00, 0.28, ""),
    (106, "Smash Burger", "entrees", "entree", 30.50, 0.34, ""),
    (107, "Fried Chicken Sandwich", "entrees", "entree", 27.50, 0.33, ""),
    (108, "Cacio e Pepe", "entrees", "entree", 31.50, 0.26, ""),
    (109, "Blackened Catfish", "entrees", "entree", 38.50, 0.34, ""),
    (110, "Harvest Grain Bowl", "entrees", "entree", 26.50, 0.30, ""),
    # sides — thin margin
    (201, "Truffle Fries", "sides", "add_on", 16.00, 0.52, ""),
    (202, "Mac and Cheese", "sides", "add_on", 14.50, 0.56, ""),
    (203, "Garlic Mashed Potatoes", "sides", "add_on", 12.00, 0.57, ""),
    (204, "Charred Broccolini", "sides", "add_on", 13.00, 0.55, ""),
    (205, "House Salad", "sides", "add_on", 11.00, 0.54, ""),
    (206, "Cornbread", "sides", "add_on", 9.50, 0.56, ""),
    (207, "Onion Rings", "sides", "add_on", 12.00, 0.55, ""),
    # beverages — fat margin, and the only category with a subtype split.
    # The alcoholic/non_alcoholic divide is what gives the drill its fourth
    # level, and it pairs with the delivery signal in Task 4: alcohol cannot
    # ship, so delivery's beverage gap lands almost entirely on this branch.
    (301, "Fountain Soda", "beverages", "add_on", 7.00, 0.09, "non_alcoholic"),
    (302, "Iced Tea", "beverages", "add_on", 6.50, 0.08, "non_alcoholic"),
    (303, "Draft Beer", "beverages", "add_on", 13.00, 0.22, "alcoholic"),
    (304, "House Wine", "beverages", "add_on", 20.50, 0.24, "alcoholic"),
    (305, "Cold Brew", "beverages", "add_on", 8.50, 0.14, "non_alcoholic"),
    (306, "Sparkling Water", "beverages", "add_on", 6.00, 0.10, "non_alcoholic"),
    (307, "Lemonade", "beverages", "add_on", 7.50, 0.11, "non_alcoholic"),
    (308, "Old Fashioned", "beverages", "add_on", 18.00, 0.21, "alcoholic"),
    (309, "Margarita", "beverages", "add_on", 17.00, 0.23, "alcoholic"),
    (310, "Negroni", "beverages", "add_on", 18.00, 0.22, "alcoholic"),
    # desserts — the deliberate null
    (401, "Chocolate Torte", "desserts", "add_on", 16.50, 0.38, ""),
    (402, "Key Lime Pie", "desserts", "add_on", 14.50, 0.37, ""),
    (403, "Seasonal Sorbet", "desserts", "add_on", 12.00, 0.36, ""),
    (404, "Sticky Toffee Pudding", "desserts", "add_on", 15.50, 0.40, ""),
    # appetizers — small real gap
    (501, "Crispy Calamari", "appetizers", "add_on", 26.00, 0.42, ""),
    (502, "Burrata and Tomato", "appetizers", "add_on", 24.00, 0.40, ""),
    (503, "Wings", "appetizers", "add_on", 22.00, 0.41, ""),
    (504, "Deviled Eggs", "appetizers", "add_on", 16.50, 0.36, ""),
    (505, "Soup of the Day", "appetizers", "add_on", 13.50, 0.34, ""),
]
# Cocktails (301-307 are the pre-existing lineup) are ordered less often than
# soda/tea/beer/wine in the generator's random draws — most beverage attaches
# are still a soft drink or a house pour, not a mixed drink. Without this,
# three new items sharing equal odds with the rest pulls the average beverage
# price up ~24% and flips the revenue ranking onto beverages, breaking the
# sides-wins-revenue / beverages-wins-profit inversion the fixture exists to
# demonstrate. See generate().
COCKTAIL_MENU_IDS = {308, 309, 310}
# Empirically tuned to SEED, not derived from any real-world ratio: this is the
# smallest weight found by trial that keeps cocktails a minority beverage draw
# without re-widening the sides ATTACH spread further than necessary. If SEED,
# ATTACH, or N_CHECKS change, re-run --check and re-verify the paired-SE
# assertion below before trusting this value again — it was fit to the
# specific draw this SEED produces, not to a target attach share.
COCKTAIL_WEIGHT = 0.3

# ── Staff roster ────────────────────────────────────────────────────────────
#
# 15 servers per location, banded by tenure. This is scaffolding for Task 4's
# "servers under 3 months don't upsell" opportunity — Task 3 only builds the
# roster, the CSV, and the join; the behavioural signal is planted later.
SERVERS_PER_LOCATION = 15
# Four bands, well under the cardinality ceiling. Weights are identical at every
# location so tenure stays independent of region — see the assertion in check().
TENURE_BANDS = ("under_3mo", "3_12mo", "1_3yr", "over_3yr")
TENURE_WEIGHTS = (0.18, 0.27, 0.31, 0.24)

FIRST_NAMES = [
    "Ana", "Ben", "Cleo", "Dev", "Elle", "Finn", "Gia", "Hugo", "Iris", "Jonah",
    "Kai", "Lena", "Milo", "Nora", "Omar", "Pia", "Quinn", "Rosa", "Sam", "Tess",
]
LAST_INITIALS = list("ABCDEFGHIJKLMNOPQRSTUVWXYZ")

# ── The signal ──────────────────────────────────────────────────────────────
#
# Attach rate = P(a check contains at least one item from this category).
# (benchmark, midwest). Entrees are ~1.0 everywhere: a check is a meal.
#
# The midwest deficits are the story. Read them as "the midwest kitchens are
# not upselling", which is a thing a regional manager can be handed and act on.
ATTACH = {
    "entrees": (1.00, 1.00),
    # Cocktails (added in 80a2d7dcc) inflated beverage revenue enough that the
    # sides-vs-beverages revenue-gap margin fell to ~1.1 SE (see COCKTAIL_WEIGHT
    # below) — a coin flip away from a different SEED silently flipping the
    # ranking. The seg rate here is widened further, from 0.37, to size the
    # headline add-on gap to a 34.0% midwest deficit ($13.715/check) while keeping
    # real headroom (not a bare pass) on the paired-SE assertion in check().
    "sides": (0.62, 0.30),  # biggest revenue gap
    # Widened from 0.56 in lockstep with sides so the profit-margin z (beverages
    # gap - sides gap) keeps pace: pushing sides' deficit alone to hit the
    # add-on-gap target would inflate sides' *profit* gap too and erode this
    # margin toward the ranking flipping onto sides — see check().
    "beverages": (0.71, 0.50),  # biggest profit gap
    "appetizers": (0.34, 0.24),  # small but real
    "desserts": (0.28, 0.28),  # deliberate null
}

# Entrees carry a small honest price gap (cheaper item mix in the midwest) so
# the top-level split is not a suspicious exact zero. ~$0.54/check against
# add-ons' ~$4.44 keeps the drill descending into add-ons.
ENTREE_CHEAP_SKEW = 0.035

DAYPARTS = ("lunch", "dinner", "late_night")
DAYPART_MIX = (0.34, 0.56, 0.10)

# How the order was placed. Independent of region/tenure/daypart by
# construction — every axis below is drawn from its own weighted choice, none
# conditioned on another, so each Task 4 signal can be asserted in isolation.
CHANNELS = ("dine_in", "takeout", "delivery", "catering", "kiosk")
CHANNEL_MIX = (0.55, 0.18, 0.15, 0.07, 0.05)

# Seating zone the order was rung from (the POS terminal zone — it applies to
# takeout and delivery tickets too, which is why it stays independent of
# channel). Deliberately carries no effect on any measure: see the
# table_section null assertion in check().
TABLE_SECTIONS = ("main", "patio", "bar")
TABLE_SECTION_MIX = (0.62, 0.23, 0.15)

# Multiplicative attach-rate modifiers, applied on top of the base ATTACH
# rates. Each key is an independent axis, so a check that is both delivery AND
# rung by a new server gets both. Independence is what lets each signal be
# asserted separately in check().
#
# delivery: alcohol is excluded outright below (it cannot ship), and the
# multipliers here additionally thin out the rest of the beverage/food attach
# — a delivery order is one fewer round of upselling than a table visit.
# under_3mo: new servers don't push apps/desserts yet.
# late_night: a smaller, real deficit — the kitchen is winding down.
# BROKEN_LOCATION_ID: one location's line cooks/bar are simply behind.
ATTACH_MODIFIERS = {
    "channel": {
        "delivery": {"beverages": 0.42, "sides": 0.80, "appetizers": 0.55, "desserts": 0.70},
        "takeout": {"beverages": 0.70, "appetizers": 0.85},
        "catering": {"beverages": 0.90},
    },
    "tenure": {
        "under_3mo": {"appetizers": 0.62, "desserts": 0.58},
    },
    "daypart": {
        "late_night": {"sides": 0.82, "appetizers": 0.80},
    },
    "location": {
        BROKEN_LOCATION_ID: {"sides": 0.55, "beverages": 0.70, "appetizers": 0.60},
    },
}

# Third-party delivery commission, taken as a discount off the ticket.
# Deliberately NOT netted into total_amount/net_revenue — see design note on
# discount_amount in checks.view.yml.
DELIVERY_DISCOUNT_RATE = 0.18
PROMO_DISCOUNT_RATE = 0.05
PROMO_SHARE = 0.08


def _menu_by_category() -> dict[str, list[tuple]]:
    by_cat: dict[str, list[tuple]] = defaultdict(list)
    for row in MENU:
        by_cat[row[2]].append(row)
    return by_cat


def _apportion(weights: tuple[float, ...], total: int) -> list[int]:
    """Largest-remainder apportionment of `total` into `len(weights)` integer
    buckets proportional to `weights`. Deterministic — no randomness — so every
    location gets an identical band composition; see the roster comment in
    generate() and the region-independence assertion in check()."""
    raw = [w * total for w in weights]
    counts = [int(x) for x in raw]
    remainder = total - sum(counts)
    order = sorted(range(len(weights)), key=lambda i: raw[i] - counts[i], reverse=True)
    for i in order[:remainder]:
        counts[i] += 1
    return counts


def generate(anchor: dt.date) -> dict[str, list[list]]:
    rng = random.Random(SEED)
    by_cat = _menu_by_category()
    cheap_entrees = sorted(by_cat["entrees"], key=lambda r: r[4])[:4]
    bev_items = by_cat["beverages"]
    bev_weights = [COCKTAIL_WEIGHT if row[0] in COCKTAIL_MENU_IDS else 1.0 for row in bev_items]

    start = anchor - dt.timedelta(days=DAYS - 1)
    region_of = {loc[0]: loc[2] for loc in LOCATIONS}
    # Traffic weights: the fleet is not uniform, but weights are independent of
    # region so midwest's deficit is attach rate, not footfall.
    loc_weight = {loc[0]: rng.uniform(0.7, 1.3) for loc in LOCATIONS}
    loc_ids = [loc[0] for loc in LOCATIONS]
    weights = [loc_weight[i] for i in loc_ids]

    # Staff roster. Drawn once, up front. Every location gets the exact same
    # *composition* of bands (via largest-remainder apportionment of
    # TENURE_WEIGHTS over SERVERS_PER_LOCATION), only shuffled into a random
    # serving order — an independent weighted draw per server (rng.choices)
    # was tried first and rejected: with only 15 draws per location it produced
    # per-location splits as skewed as 9/15 in one band, which blew the
    # region-independence tolerance in check() by a wide margin. Fixing the
    # composition and randomizing only the order keeps every location
    # identical, so tenure is independent of region by construction, not by
    # hoping 15 samples converge to TENURE_WEIGHTS.
    band_counts = _apportion(TENURE_WEIGHTS, SERVERS_PER_LOCATION)
    servers: list[list] = []
    servers_by_location: dict[int, list[int]] = defaultdict(list)
    server_id = 5_000
    for loc in LOCATIONS:
        band_list = [band for band, cnt in zip(TENURE_BANDS, band_counts) for _ in range(cnt)]
        rng.shuffle(band_list)
        for band in band_list:
            server_id += 1
            # hire_date is derived from the band so the two never disagree.
            max_days_ago = {"under_3mo": 90, "3_12mo": 365, "1_3yr": 1095, "over_3yr": 3650}[band]
            min_days_ago = {"under_3mo": 0, "3_12mo": 91, "1_3yr": 366, "over_3yr": 1096}[band]
            hired = anchor - dt.timedelta(days=rng.randint(min_days_ago, max_days_ago))
            name = f"{rng.choice(FIRST_NAMES)} {rng.choice(LAST_INITIALS)}."
            servers.append([server_id, name, loc[0], hired.isoformat(), band])
            servers_by_location[loc[0]].append(server_id)
    server_band = {row[0]: row[4] for row in servers}

    checks: list[list] = []
    items: list[list] = []
    check_id = 700_000

    for _ in range(N_CHECKS):
        check_id += 1
        loc_id = rng.choices(loc_ids, weights=weights, k=1)[0]
        is_segment = region_of[loc_id] == SEGMENT_REGION
        day = start + dt.timedelta(days=rng.randrange(DAYS))
        daypart = rng.choices(DAYPARTS, weights=DAYPART_MIX, k=1)[0]
        party_size = rng.choices((1, 2, 3, 4, 5, 6), weights=(0.14, 0.42, 0.16, 0.18, 0.06, 0.04), k=1)[0]

        # Server is drawn here (moved up from after the add-on loop, where
        # Task 3 first put it) because the add-on attach loop below needs
        # tenure_band to apply the under_3mo modifier. This reshuffles the RNG
        # stream for every downstream draw on this check relative to Task 3's
        # dataset — expected and harmless, since SEED/ATTACH/menu are unchanged
        # and `--check` re-verifies every invariant and guard statistic against
        # whatever comes out the other end.
        server = rng.choice(servers_by_location[loc_id])
        tenure = server_band[server]
        channel = rng.choices(CHANNELS, weights=CHANNEL_MIX, k=1)[0]
        section = rng.choices(TABLE_SECTIONS, weights=TABLE_SECTION_MIX, k=1)[0]

        line_id = 0
        total = 0.0

        # Entrees: one per guest. The midwest skews toward the cheaper end of
        # the menu, which is a mix effect, not an attach effect.
        for _ in range(party_size):
            if is_segment and rng.random() < ENTREE_CHEAP_SKEW:
                item = rng.choice(cheap_entrees)
            else:
                item = rng.choice(by_cat["entrees"])
            line_id += 1
            items.append([check_id, line_id, item[0], 1])
            total += item[4]

        # Add-ons: attach is per check, quantity scales loosely with party
        # size. Base ATTACH rate is thinned by every independent modifier axis
        # that applies to this check (channel/tenure/daypart/location) — see
        # ATTACH_MODIFIERS.
        for category in ("sides", "beverages", "appetizers", "desserts"):
            bench_rate, seg_rate = ATTACH[category]
            rate = seg_rate if is_segment else bench_rate
            rate *= ATTACH_MODIFIERS["channel"].get(channel, {}).get(category, 1.0)
            rate *= ATTACH_MODIFIERS["tenure"].get(tenure, {}).get(category, 1.0)
            rate *= ATTACH_MODIFIERS["daypart"].get(daypart, {}).get(category, 1.0)
            rate *= ATTACH_MODIFIERS["location"].get(loc_id, {}).get(category, 1.0)
            if rng.random() >= rate:
                continue
            if category == "beverages":
                qty = max(1, min(party_size, rng.randint(party_size - 1, party_size + 1)))
            elif category == "sides":
                qty = max(1, round(party_size * rng.uniform(0.4, 0.9)))
            else:
                qty = max(1, round(party_size * rng.uniform(0.25, 0.6)))
            # Alcohol cannot ship. This is what makes the delivery gap resolve
            # all the way down to the alcoholic branch rather than stopping at
            # "beverages".
            pool = by_cat[category]
            if category == "beverages":
                if channel == "delivery":
                    item = rng.choice([r for r in pool if r[6] != "alcoholic"])
                else:
                    item = rng.choices(bev_items, weights=bev_weights, k=1)[0]
            else:
                item = rng.choice(pool)
            line_id += 1
            items.append([check_id, line_id, item[0], qty])
            total += item[4] * qty

        total = round(total, 2)

        # Third-party delivery commission, or an occasional promo discount.
        # Deliberately kept off of total_amount/net_revenue — see
        # discount_amount's description in checks.view.yml.
        if channel == "delivery":
            discount = round(total * DELIVERY_DISCOUNT_RATE, 2)
        elif rng.random() < PROMO_SHARE:
            discount = round(total * PROMO_DISCOUNT_RATE, 2)
        else:
            discount = 0.0

        checks.append(
            [
                check_id,
                loc_id,
                day.isoformat(),
                daypart,
                party_size,
                total,
                round(total * TAX_RATE, 2),
                server,
                channel,
                section,
                discount,
            ]
        )

    location_rows = [[i, name, region, city] for i, name, region, city in LOCATIONS]
    menu_rows = [
        [mid, name, cat, itype, round(price, 2), round(price * ratio, 2), bev_type]
        for mid, name, cat, itype, price, ratio, bev_type in MENU
    ]
    return {
        "restaurant_locations.csv": location_rows,
        "restaurant_menu_items.csv": menu_rows,
        "restaurant_checks.csv": checks,
        "restaurant_check_items.csv": items,
        "restaurant_servers.csv": servers,
    }


HEADERS = {
    "restaurant_locations.csv": ["location_id", "location_name", "region", "city"],
    "restaurant_menu_items.csv": [
        "menu_item_id",
        "item_name",
        "category",
        "item_type",
        "unit_price",
        "unit_cost",
        "beverage_type",
    ],
    "restaurant_checks.csv": [
        "check_id",
        "location_id",
        "check_date",
        "daypart",
        "party_size",
        "total_amount",
        "tax_amount",
        "server_id",
        "order_channel",
        "table_section",
        "discount_amount",
    ],
    "restaurant_check_items.csv": ["check_id", "line_item_id", "menu_item_id", "quantity"],
    "restaurant_servers.csv": ["server_id", "server_name", "location_id", "hire_date", "tenure_band"],
}


def write(tables: dict[str, list[list]]) -> None:
    DB.mkdir(parents=True, exist_ok=True)
    for fname, rows in tables.items():
        path = DB / fname
        with path.open("w", newline="") as fh:
            w = csv.writer(fh)
            w.writerow(HEADERS[fname])
            w.writerows(rows)
        print(f"wrote {path} ({len(rows)} rows)")


def _read(fname: str) -> list[dict]:
    with (DB / fname).open() as fh:
        return list(csv.DictReader(fh))


def _bench_seg_z(
    per_check_value: dict[int, float],
    checks: dict[int, dict],
    is_seg,
) -> tuple[float, float, float]:
    """Ordinary two-sample bench-vs-segment z for the mean of a per-check scalar.

    This is the shared tail end of every bench-vs-segment significance check
    in this file: split `per_check_value` (checks missing from the mapping
    count as 0, i.e. "attached nothing relevant") into a benchmark sample
    (`not is_seg(c)`) and a segment sample (`is_seg(c)`) and compare means.
    `is_seg` is an arbitrary predicate over a check row — every caller so far
    partitions on `region == SEGMENT_REGION`, but nothing here is
    region-specific; the table_section null guard reuses this same tail with
    a table_section predicate over a `checks` dict pre-filtered to the two
    sections being compared. What varies between callers is only how
    `per_check_value` was built — a single category's per-check amount
    (`_category_gap_z`) or a paired difference between two categories
    (`_paired_gap_z`) — the mean/variance/SE/z arithmetic itself is identical
    either way, so it lives here once. Returns (diff, se, z) where diff is
    bench_mean - seg_mean.
    """
    d_bench = [per_check_value.get(cid, 0.0) for cid, c in checks.items() if not is_seg(c)]
    d_seg = [per_check_value.get(cid, 0.0) for cid, c in checks.items() if is_seg(c)]

    def _mean(xs: list[float]) -> float:
        return sum(xs) / len(xs)

    def _var(xs: list[float]) -> float:
        m = _mean(xs)
        return sum((x - m) ** 2 for x in xs) / (len(xs) - 1)

    diff = _mean(d_bench) - _mean(d_seg)
    se = math.sqrt(_var(d_bench) / len(d_bench) + _var(d_seg) / len(d_seg))
    z = diff / se if se else float("inf")
    return diff, se, z


def _paired_gap_z(
    items: list[dict],
    checks: dict[int, dict],
    locations: dict[int, dict],
    menu: dict[int, dict],
    cat_a: str,
    cat_b: str,
    value,
) -> tuple[float, float, float]:
    """Paired bench-vs-segment z for the (cat_a - cat_b) per-check gap.

    ``value(menu_row)`` extracts the per-unit amount to compare — unit_price
    for a revenue margin, unit_price - unit_cost for a profit margin. Builds
    the per-check difference d = value(cat_a) - value(cat_b) (0 for a check
    that attaches neither category): cat_a's and cat_b's gaps are correlated
    (the same check's location and party size drive both), so the combined SE
    has to come from the variance of d itself, not from adding the two
    categories' gap variances as if they were independent. The rest of the
    test (mean/variance/SE/z) is shared with `_category_gap_z` via
    `_bench_seg_z`. Returns (diff, se, z).
    """
    per_check_diff: dict[int, float] = defaultdict(float)
    for i in items:
        m = menu[int(i["menu_item_id"])]
        if m["category"] not in (cat_a, cat_b):
            continue
        amount = value(m) * int(i["quantity"])
        per_check_diff[int(i["check_id"])] += amount if m["category"] == cat_a else -amount

    def is_seg(c: dict) -> bool:
        return locations[int(c["location_id"])]["region"] == SEGMENT_REGION

    return _bench_seg_z(per_check_diff, checks, is_seg)


def _category_gap_z(
    items: list[dict],
    checks: dict[int, dict],
    locations: dict[int, dict],
    menu: dict[int, dict],
    category: str,
    value,
) -> tuple[float, float, float]:
    """Bench-vs-segment z for a single category's per-check value.

    Unlike `_paired_gap_z`, there is no second category to net against here —
    this is for a category (desserts) whose gap is supposed to be a null
    against the *benchmark*, not against a sibling category. Same
    mean/variance/SE/z tail as `_paired_gap_z`, via `_bench_seg_z`; only the
    per-check value being tested differs (one category's amount instead of a
    paired difference). Returns (diff, se, z).
    """
    per_check_value: dict[int, float] = defaultdict(float)
    for i in items:
        m = menu[int(i["menu_item_id"])]
        if m["category"] != category:
            continue
        per_check_value[int(i["check_id"])] += value(m) * int(i["quantity"])

    def is_seg(c: dict) -> bool:
        return locations[int(c["location_id"])]["region"] == SEGMENT_REGION

    return _bench_seg_z(per_check_value, checks, is_seg)


def _addon_revenue_gap(
    items: list[dict],
    checks: dict[int, dict],
    menu: dict[int, dict],
    is_seg,
) -> tuple[float, int, int]:
    """Per-check add-on revenue gap (benchmark mean - segment mean) for an
    arbitrary check-level segment predicate `is_seg(check_row) -> bool`.

    This is the measurement tool for Task 4's four new signals
    (channel/tenure/daypart/location), reported by check() so Task 5 can set
    its assertion floors from real numbers rather than estimates. It
    deliberately does not touch `_bench_seg_z` (which stays hardcoded to
    SEGMENT_REGION for the four guarded statistics above) — this is a
    reporting helper, not a guard.
    """
    addon_cats = ("sides", "beverages", "appetizers", "desserts")
    per_check: dict[int, float] = defaultdict(float)
    for i in items:
        m = menu[int(i["menu_item_id"])]
        if m["category"] in addon_cats:
            per_check[int(i["check_id"])] += int(i["quantity"]) * float(m["unit_price"])
    seg_ids = [cid for cid, c in checks.items() if is_seg(c)]
    bench_ids = [cid for cid, c in checks.items() if not is_seg(c)]
    seg_mean = sum(per_check.get(cid, 0.0) for cid in seg_ids) / len(seg_ids)
    bench_mean = sum(per_check.get(cid, 0.0) for cid in bench_ids) / len(bench_ids)
    return bench_mean - seg_mean, len(seg_ids), len(bench_ids)


def check() -> int:
    """Verify invariants and the intended revenue/profit inversion on disk."""
    failures: list[str] = []
    menu = {int(r["menu_item_id"]): r for r in _read("restaurant_menu_items.csv")}
    locations = {int(r["location_id"]): r for r in _read("restaurant_locations.csv")}
    checks = {int(r["check_id"]): r for r in _read("restaurant_checks.csv")}
    items = _read("restaurant_check_items.csv")
    servers = {int(r["server_id"]): r for r in _read("restaurant_servers.csv")}

    # Cardinality ceiling. Asserted directly rather than inferred from
    # len(LOCATIONS): the engine skips any dimension at 25+ distinct values as a
    # "high-cardinality identifier", so a future edit that grows the fleet must
    # fail here rather than silently deleting location_name (or any other axis
    # Task 4 added) as an opportunity axis. Covers every dimension this fixture
    # segments on, not just the three original ones.
    CARDINALITY_CEILING = 25
    dim_cardinalities = {
        "location_name": len({r["location_name"] for r in locations.values()}),
        "region": len({r["region"] for r in locations.values()}),
        "city": len({r["city"] for r in locations.values()}),
        "order_channel": len({c["order_channel"] for c in checks.values()}),
        "tenure_band": len({r["tenure_band"] for r in servers.values()}),
        "table_section": len({c["table_section"] for c in checks.values()}),
    }
    for dim, n in sorted(dim_cardinalities.items()):
        if n >= CARDINALITY_CEILING:
            failures.append(
                f"dimension {dim} has {n} distinct values, at or above the engine's "
                f"cardinality ceiling of {CARDINALITY_CEILING} — it will be skipped as a "
                f"high-cardinality identifier and stop being an opportunity axis"
            )

    # The scan period presets run 30/90/180/365 days back from *yesterday*. Data
    # must cover the longest preset with margin, or the default view has empty
    # days at its right edge.
    dates = sorted(c["check_date"] for c in checks.values())
    span = (dt.date.fromisoformat(dates[-1]) - dt.date.fromisoformat(dates[0])).days + 1
    if span < 366:
        failures.append(f"data spans {span} days, need >= 366 to cover the 365d preset")

    # checks.view.yml computes entree_revenue off `item_type = 'entree'`, but
    # every other identity check below (and every other revenue bucket) keys
    # off `category`. The two only agree because MENU happens to be populated
    # that way by hand: every "entrees" row is item_type "entree" and nothing
    # else is. Assert that equivalence directly against MENU rather than
    # trusting the convention — if someone later adds, say, an `add_on` item
    # under category "entrees" (or an "entree"-typed item outside it),
    # `--check` would otherwise keep passing while net_revenue no longer
    # equals entree_revenue + addon_revenue in the compiled SQL, and the
    # drill's per-category concentrations stop summing to 1.
    menu_identity_mismatches = [
        (mid, name, cat, itype)
        for mid, name, cat, itype, _price, _cost_ratio, _bev in MENU
        if (itype == "entree") != (cat == "entrees")
    ]
    if menu_identity_mismatches:
        failures.append(
            f"{len(menu_identity_mismatches)} MENU rows where item_type == 'entree' does not "
            f"match category == 'entrees': {menu_identity_mismatches}"
        )

    # Referential integrity
    orphan_checks = {int(i["check_id"]) for i in items} - set(checks)
    orphan_items = {int(i["menu_item_id"]) for i in items} - set(menu)
    if orphan_checks:
        failures.append(f"{len(orphan_checks)} check_items reference a missing check")
    if orphan_items:
        failures.append(f"{len(orphan_items)} check_items reference a missing menu item")

    # Tenure must be independent of region, or the midwest headline and the
    # tenure signal contaminate each other: a midwest skewed toward new servers
    # would make the regional add-on gap partly a staffing artifact, and the
    # desserts null would stop being null on the region axis.
    band_by_region: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for c in checks.values():
        region = locations[int(c["location_id"])]["region"]
        band_by_region[region][servers[int(c["server_id"])]["tenure_band"]] += 1
    for band in TENURE_BANDS:
        shares = [
            band_by_region[r][band] / sum(band_by_region[r].values()) for r in band_by_region
        ]
        if max(shares) - min(shares) > 0.04:
            failures.append(
                f"tenure_band {band} share ranges {min(shares):.3f}..{max(shares):.3f} across "
                f"regions — must be within 0.04 or tenure confounds the region signal"
            )

    orphan_servers = {int(c["server_id"]) for c in checks.values()} - set(servers)
    if orphan_servers:
        failures.append(f"{len(orphan_servers)} checks reference a missing server")

    # total_amount == SUM(quantity * unit_price); tax == round(total * rate, 2)
    recomputed: dict[int, float] = defaultdict(float)
    for i in items:
        recomputed[int(i["check_id"])] += int(i["quantity"]) * float(menu[int(i["menu_item_id"])]["unit_price"])
    bad_total = sum(1 for cid, c in checks.items() if abs(round(recomputed[cid], 2) - float(c["total_amount"])) > 0.011)
    bad_tax = sum(
        1 for c in checks.values() if abs(round(float(c["total_amount"]) * TAX_RATE, 2) - float(c["tax_amount"])) > 0.011
    )
    if bad_total:
        failures.append(f"{bad_total} checks where total_amount != SUM(quantity * unit_price)")
    if bad_tax:
        failures.append(f"{bad_tax} checks where tax_amount != round(total_amount * {TAX_RATE}, 2)")

    # Additive identity checks.view.yml depends on: net_revenue decomposes as
    # entree_revenue + addon_revenue, and addon_revenue as sides + beverages +
    # appetizers + desserts. Recompute each category's revenue per check from
    # the line items and confirm the pieces reconstruct total_amount to the
    # cent — independent of the flat SUM(quantity * unit_price) check above.
    cat_revenue: dict[int, dict[str, float]] = defaultdict(lambda: defaultdict(float))
    for i in items:
        m = menu[int(i["menu_item_id"])]
        cat_revenue[int(i["check_id"])][m["category"]] += int(i["quantity"]) * float(m["unit_price"])
    identity_cats = ("entrees", "sides", "beverages", "appetizers", "desserts")
    bad_identity = [
        (cid, sum(cat_revenue[cid].get(cat, 0.0) for cat in identity_cats), float(c["total_amount"]))
        for cid, c in checks.items()
    ]
    bad_identity = [
        (cid, recon, total) for cid, recon, total in bad_identity if abs(round(recon, 2) - total) > 0.011
    ]
    if bad_identity:
        max_dev = max(abs(round(recon, 2) - total) for _, recon, total in bad_identity)
        sample = bad_identity[:5]
        failures.append(
            f"{len(bad_identity)} checks where entree_revenue + sides + beverages + "
            f"appetizers + desserts != total_amount (max dev {max_dev:.4f}, sample {sample})"
        )

    # Third component level: beverages must reconstruct exactly from its two
    # subtypes, or `beverages_revenue = alcoholic + na_beverage` stops being an
    # arithmetic identity and the drill's concentration shares at that level
    # become fiction.
    bev_recon: dict[int, float] = defaultdict(float)
    bev_total: dict[int, float] = defaultdict(float)
    for i in items:
        m = menu[int(i["menu_item_id"])]
        if m["category"] != "beverages":
            continue
        line = int(i["quantity"]) * float(m["unit_price"])
        bev_total[int(i["check_id"])] += line
        if m["beverage_type"] in ("alcoholic", "non_alcoholic"):
            bev_recon[int(i["check_id"])] += line
        else:
            failures.append(
                f"menu item {i['menu_item_id']} is a beverage with beverage_type "
                f"{m['beverage_type']!r}, expected 'alcoholic' or 'non_alcoholic'"
            )
    bad_bev = [
        cid for cid in bev_total if abs(round(bev_recon[cid], 2) - round(bev_total[cid], 2)) > 0.011
    ]
    if bad_bev:
        failures.append(
            f"{len(bad_bev)} checks where alcoholic + na_beverage != beverages_revenue"
        )

    # The inversion: revenue ranks sides first, profit ranks beverages first.
    seg_n = sum(1 for c in checks.values() if locations[int(c["location_id"])]["region"] == SEGMENT_REGION)
    bench_n = len(checks) - seg_n
    rev: dict[str, list[float]] = defaultdict(lambda: [0.0, 0.0])
    prof: dict[str, list[float]] = defaultdict(lambda: [0.0, 0.0])
    for i in items:
        m = menu[int(i["menu_item_id"])]
        c = checks[int(i["check_id"])]
        idx = 0 if locations[int(c["location_id"])]["region"] == SEGMENT_REGION else 1
        qty = int(i["quantity"])
        rev[m["category"]][idx] += qty * float(m["unit_price"])
        prof[m["category"]][idx] += qty * (float(m["unit_price"]) - float(m["unit_cost"]))

    addon_cats = [c for c in rev if c != "entrees"]
    rev_gap = {c: rev[c][1] / bench_n - rev[c][0] / seg_n for c in addon_cats}
    prof_gap = {c: prof[c][1] / bench_n - prof[c][0] / seg_n for c in addon_cats}
    top_rev = max(rev_gap, key=lambda c: rev_gap[c])
    top_prof = max(prof_gap, key=lambda c: prof_gap[c])

    print(f"\n  segment={SEGMENT_REGION} n={seg_n}  benchmark n={bench_n}")
    print(f"  {'category':<12} {'rev gap/chk':>12} {'profit gap/chk':>15}")
    for c in sorted(rev_gap, key=lambda c: -rev_gap[c]):
        print(f"  {c:<12} {rev_gap[c]:>12.3f} {prof_gap[c]:>15.3f}")
    entree_gap = rev["entrees"][1] / bench_n - rev["entrees"][0] / seg_n
    addon_gap = sum(rev_gap.values())
    print(f"\n  entree gap/chk {entree_gap:.3f} vs add-on gap/chk {addon_gap:.3f}")
    print(f"  top revenue gap = {top_rev}; top profit gap = {top_prof}")

    # The two checks above only assert the *ranking* (top_rev == "sides"), and
    # that passes identically whether the margin is 33 SE or 1.1 SE — it is
    # exactly the ranking check that kept passing while 80a2d7dcc (cocktails)
    # quietly eroded the margin from ~40% to ~5%. Assert the margin itself, not
    # just its sign, via the shared paired-z helper (see _paired_gap_z).
    MIN_MARGIN_Z = 4.0
    rev_diff, rev_se, rev_z = _paired_gap_z(
        items, checks, locations, menu, "sides", "beverages", lambda m: float(m["unit_price"])
    )
    print(f"  revenue-margin z (sides gap - beverages gap) = {rev_z:.2f}  (diff {rev_diff:.3f}, SE {rev_se:.3f})")

    # The revenue gate's own history is the reason the profit side needs the
    # same treatment: top_prof != "beverages" below is a pure ranking check,
    # blind to margin size, and a ranking check cannot tell a robust inversion
    # from a coin flip — that blind spot is exactly how the revenue margin
    # above eroded to ~1 SE unnoticed. Task 4 adds a
    # delivery channel that carries no alcohol, which will pull directly on
    # beverages' profit gap; gate on the margin now so that erosion fails
    # loudly here instead of sailing through on a ranking that still happens
    # to hold.
    prof_diff, prof_se, prof_z = _paired_gap_z(
        items, checks, locations, menu, "beverages", "sides", lambda m: float(m["unit_price"]) - float(m["unit_cost"])
    )
    print(f"  profit-margin z (beverages gap - sides gap) = {prof_z:.2f}  (diff {prof_diff:.3f}, SE {prof_se:.3f})")

    # This is the assertion that actually protects the demo: the ranking gates
    # below are blind to margin, so a SEED/ATTACH/menu edit that leaves the
    # ranking intact but shrinks the gap toward a coin flip would sail through
    # them. 4 SE leaves headroom under the margins this fixture is tuned to
    # (~15.71 SE revenue, ~5 SE profit), so ordinary regeneration noise (there is
    # none — SEED is fixed — but future edits) doesn't false-fail right at the
    # boundary.
    if rev_z < MIN_MARGIN_Z:
        failures.append(
            f"sides-vs-beverages revenue-gap margin is only {rev_z:.2f} SE "
            f"(diff {rev_diff:.3f}, SE {rev_se:.3f}), need >= {MIN_MARGIN_Z} SE — "
            f"the ranking check below (top_rev == 'sides') passes just as happily at "
            f"1 SE as at 30 SE, which is exactly how this margin eroded unnoticed before"
        )
    if prof_z < MIN_MARGIN_Z:
        failures.append(
            f"beverages-vs-sides profit-gap margin is only {prof_z:.2f} SE "
            f"(diff {prof_diff:.3f}, SE {prof_se:.3f}), need >= {MIN_MARGIN_Z} SE — "
            f"the ranking check below (top_prof == 'beverages') passes just as happily at "
            f"1 SE as at 30 SE, the same blind spot that let the revenue margin erode before"
        )

    if top_rev != "sides":
        failures.append(f"expected sides to own the largest revenue gap, got {top_rev}")
    if top_prof != "beverages":
        failures.append(f"expected beverages to own the largest profit gap, got {top_prof}")
    # No separate top_rev == top_prof assertion: the two checks above already
    # pin top_rev to "sides" and top_prof to "beverages", so if both pass the
    # values can never be equal — a third check for that would be unreachable.
    if addon_gap <= entree_gap * 4:
        failures.append(f"add-on gap ({addon_gap:.3f}) is not dominant over entrees ({entree_gap:.3f})")

    # The desserts null used to be gated on a flat $0.05 threshold. A dollar
    # amount only holds still if sample size and price level hold still too —
    # neither did: the fixture grew from ~45k checks to 104k (SE shrinks) and
    # menu prices rose ~1.85x (gap and SE both grow) since that threshold was
    # set, and the measured gap drifted to 0.70 SE / 1.12 SE of coin-flip
    # noise while the fixed $0.05 stayed put, silently decalibrating the gate
    # without any code change telling you so. Expressing it as a multiple of
    # the measured SE (via `_category_gap_z`, reusing the same bench-vs-segment
    # z machinery `_paired_gap_z` uses) makes the threshold self-scale with
    # whatever the fixture's sample size and prices happen to be on any given
    # run, including the new per-check draws Task 4 adds.
    NULL_MAX_Z = 3.0
    rev_null_diff, rev_null_se, rev_null_z = _category_gap_z(
        items, checks, locations, menu, "desserts", lambda m: float(m["unit_price"])
    )
    print(
        f"  desserts null revenue z = {rev_null_z:.2f}  (gap {rev_null_diff:.3f}, SE {rev_null_se:.3f})"
    )
    prof_null_diff, prof_null_se, prof_null_z = _category_gap_z(
        items, checks, locations, menu, "desserts", lambda m: float(m["unit_price"]) - float(m["unit_cost"])
    )
    print(
        f"  desserts null profit z  = {prof_null_z:.2f}  (gap {prof_null_diff:.3f}, SE {prof_null_se:.3f})"
    )
    if abs(rev_null_z) >= NULL_MAX_Z:
        failures.append(
            f"desserts should be a null on revenue, z={rev_null_z:.2f} "
            f"(gap {rev_null_diff:.3f}, SE {rev_null_se:.3f}) is >= {NULL_MAX_Z} SE — "
            f"attach rates for the two groups have drifted apart, not just sampling noise"
        )
    if abs(prof_null_z) >= NULL_MAX_Z:
        failures.append(
            f"desserts should be a null on profit, z={prof_null_z:.2f} "
            f"(gap {prof_null_diff:.3f}, SE {prof_null_se:.3f}) is >= {NULL_MAX_Z} SE — "
            f"attach rates for the two groups have drifted apart, not just sampling noise"
        )

    # Delivery: no alcohol at all, and a thin beverage attach generally. This is
    # the signal that gives the drill its full four-level descent.
    chan_alc: dict[str, float] = defaultdict(float)
    for i in items:
        m = menu[int(i["menu_item_id"])]
        if m["beverage_type"] == "alcoholic":
            chan_alc[checks[int(i["check_id"])]["order_channel"]] += (
                int(i["quantity"]) * float(m["unit_price"])
            )
    if chan_alc["delivery"] != 0.0:
        failures.append(
            f"delivery has {chan_alc['delivery']:.2f} of alcohol revenue, expected exactly 0"
        )

    # table_section is a deliberate null: a dimension with no relationship to
    # the measure at all. Its job is to be DROPPED by the significance gate and
    # counted in segments_dropped_as_noise. A fixture where every dimension is
    # significant cannot tell a working gate from one that rubber-stamps.
    #
    # Gated as a z-score, like every other guard here (see the module
    # docstring's "Guard statistics" section): a dollar spread threshold
    # decalibrates the same way the old $0.05 desserts floor did. Compare the
    # two most divergent sections' means directly — that IS the spread, so
    # it's the tightest test of it — reusing `_bench_seg_z`'s mean/var/SE/z
    # tail on a `checks` view filtered to just those two sections.
    sec_rev: dict[str, float] = defaultdict(float)
    sec_n: dict[str, int] = defaultdict(int)
    for c in checks.values():
        sec_rev[c["table_section"]] += float(c["total_amount"])
        sec_n[c["table_section"]] += 1
    sec_rates = {s: sec_rev[s] / sec_n[s] for s in sec_rev}
    hi_section = max(sec_rates, key=lambda s: sec_rates[s])
    lo_section = min(sec_rates, key=lambda s: sec_rates[s])
    spread = sec_rates[hi_section] - sec_rates[lo_section]

    per_check_amount = {cid: float(c["total_amount"]) for cid, c in checks.items()}
    extreme_checks = {
        cid: c for cid, c in checks.items() if c["table_section"] in (hi_section, lo_section)
    }

    def is_hi_section(c: dict) -> bool:
        return c["table_section"] == hi_section

    sec_diff, sec_se, sec_z = _bench_seg_z(per_check_amount, extreme_checks, is_hi_section)
    print(
        f"  table_section null z ({hi_section} vs {lo_section}) = {sec_z:.2f}  "
        f"(spread {spread:.3f}, diff {sec_diff:.3f}, SE {sec_se:.3f})"
    )
    TABLE_SECTION_MAX_Z = 3.0
    if abs(sec_z) >= TABLE_SECTION_MAX_Z:
        failures.append(
            f"table_section's most divergent sections ({hi_section} vs {lo_section}) differ by "
            f"z={sec_z:.2f} (spread {spread:.3f}/check, diff {sec_diff:.3f}, SE {sec_se:.3f}), "
            f">= {TABLE_SECTION_MAX_Z} SE — table_section should stay a null the significance "
            f"gate drops"
        )

    # Task 4 signal gaps: measured (not estimated) per-check add-on revenue gap
    # for each of the five opportunity signals this fixture now plants. Task 5
    # gates on these printed numbers — both the gap floor (erosion) and the
    # rank (reordering) — reusing `_addon_revenue_gap` rather than adding a
    # second copy of the same measurement.
    #
    # Upside is gap x volume, so both terms feed the ranking: a gap that
    # survives while its segment shrinks would silently drop the row off the
    # ranked list even though the per-check gap floor still holds.
    #
    # Floors are 85% of the gap each signal MEASURED after Task 4, rounded
    # down. They exist to catch EROSION, not to re-specify the target — an
    # earlier draft guessed these and was wrong by 2.5x, which would have
    # failed forever. Measured 2026-07-20 (add-on revenue $/check, benchmark
    # minus segment):
    #   midwest         13.715  (n=22,249)
    #   delivery        17.984  (n=15,519)
    #   new_server       4.957  (n=20,930)
    #   late_night       3.421  (n=10,411)
    #   broken_location 10.236  (n= 3,304)
    MIN_GAP = {
        "midwest (region)": 11.6,
        "delivery (channel)": 15.2,
        "new_server (tenure)": 4.2,
        "late_night (daypart)": 2.9,
        "broken_location": 8.7,
    }
    print("\n  Task 4 signal gaps (add-on revenue/check, benchmark - segment):")
    signals = {
        "midwest (region)": lambda c: locations[int(c["location_id"])]["region"] == SEGMENT_REGION,
        "delivery (channel)": lambda c: c["order_channel"] == "delivery",
        "new_server (tenure)": lambda c: servers[int(c["server_id"])]["tenure_band"] == "under_3mo",
        "late_night (daypart)": lambda c: c["daypart"] == "late_night",
        "broken_location": lambda c: int(c["location_id"]) == BROKEN_LOCATION_ID,
    }
    upsides: dict[str, float] = {}
    for label, pred in signals.items():
        gap, n_seg, n_bench = _addon_revenue_gap(items, checks, menu, pred)
        upsides[label] = gap * n_seg
        print(f"    {label:<22} gap={gap:>8.3f}/check  n_seg={n_seg:<7} n_bench={n_bench}")
        if gap < MIN_GAP[label]:
            failures.append(
                f"signal {label} has an add-on gap of {gap:.3f}/check, below the "
                f"{MIN_GAP[label]:.2f} floor it needs to clear the significance bar"
            )

    # The two headline signals must out-rank the three supporting ones, or the
    # panel's top rows stop telling the story the demo is built around.
    ranked = sorted(upsides, key=lambda k: -upsides[k])
    if set(ranked[:2]) != {"midwest (region)", "delivery (channel)"}:
        failures.append(
            f"expected midwest and delivery to be the top two opportunities, got {ranked[:2]}"
        )

    if failures:
        print("\nFAILED:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("\nall invariants hold")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--anchor", default=ANCHOR, help="last date in the dataset (YYYY-MM-DD)")
    ap.add_argument("--check", action="store_true", help="verify what is on disk; write nothing")
    args = ap.parse_args()

    if args.check:
        return check()

    write(generate(dt.date.fromisoformat(args.anchor)))
    return check()


if __name__ == "__main__":
    raise SystemExit(main())
