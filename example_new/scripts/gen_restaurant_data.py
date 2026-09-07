#!/usr/bin/env python3
"""Generate the restaurant demo dataset: a 24-location chain shaped so five
independent opportunity signals, a three-level revenue/profit tree, two
deliberate nulls, and every case the scenario simulator distinguishes all
coexist on the same 104,000-check fixture.

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

The store-day table, and why the check tree could not do its job
-----------------------------------------------------------------
`restaurant_store_days.csv` (24 locations x 400 days = 9,600 rows) serves a
different feature from everything above: the Metric Tree's **scenario
simulation** — pin a lever, propagate a delta forward. That engine branches on
the arithmetic operator of each edge and on whether an edge is a declared
driver, and the check-grain tree exercises exactly one of those branches: every
edge in `checks.view.yml` is a `+`, so any lever pinned there propagates
exactly and nothing else is ever observable. `store_days.view.yml` writes the
rest — a subtraction, five ratios, a constant-factor product, three
quantified drivers, one saturating driver whose coefficient is an elasticity
rather than a slope, one deliberately unquantified driver, and one edge the
engine must refuse to size. That file's header maps each case to the measure
carrying it; this file supplies the data underneath.

Three things about the data are load-bearing rather than decorative:

- **Sales, covers and food cost are not stored here.** The view re-derives
  them from the check data, so the two grains cannot drift apart.
- **Each quantified driver is constructed backwards from the outcome it
  explains** (spend from 7-day-ahead smoothed sales, redemptions from
  3-day-ahead covers, signups from 21-day-ahead redemptions), so the
  coefficients in the YAML are measurable properties of the data.
  `check()` re-measures all four by within-location OLS and fails if any
  drifts more than `DRIVER_SLOPE_TOL` from what the view declares — nothing
  in the engine validates a `drivers:` coefficient, so a declared 6.0 against
  data that says 3.0 would forecast wrong numbers forever, silently.
- **The banquet program is exactly zero for the last 120 days.** A
  multiplicative edge whose child is zero cannot be sized (%delta is undefined
  at zero), so on the scenario default (a trailing 90-day window)
  `banquet_check_average` must come back "can't size this" — which is a
  different claim from "no impact" — and must size normally once the window
  widens to 365 days.
- **One driver pair is deliberately CURVED**, built forward rather than
  backwards: `delivery_orders = scale_loc * delivery_app_spend ** 0.45`, drawn
  from its own `DELIVERY_SEED` stream so adding it left every other column
  bit-for-bit unchanged. The view declares neither a shape nor a
  coefficient, so the engine fits an elasticity (0.451 measured here) rather
  than a slope. Every other edge in this fixture is linear, and on linear data
  a fit that honours `form:` and one that ignores it return the same number —
  so without this pair, an engine that never read `form:` at all would pass
  every assertion this file makes. The same rows fitted in LEVELS give 0.109,
  and `check()` asserts the two stay far apart. ~4% of days are dark (spend 0):
  `ln(0)` is undefined, so those are the rows a log fit must drop and *report*
  rather than silently narrow its window with.

`weather_severity_index` is the driver-side twin of `table_section`: declared
on the view with direction and strength but NO coefficient, drawn independently
of everything, and asserted inert (t = 0.35 against covers). It exists so the
fixture can show a declared driver that correctly propagates *nothing*.

The size budget
------------------
Committed CSV is budgeted at ~15 MB; the six files on disk currently total
14,447,847 bytes (~14.4 MB), of which the store-day table is 574 KB — it grew
88 KB when the two delivery columns landed, which is the whole remaining
headroom's worth of a rounding error, but the budget is why a third pair would
need a justification rather than just a use.
`restaurant_checks.csv` and `restaurant_check_items.csv` (446,726 line items
for 104,000 checks) are the two files that scale with check volume, so that
headroom — not aesthetics — is why `N_CHECKS` stops at 104,000: a further
increase without shrinking something else would blow the budget.

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
- exactly one `restaurant_store_days.csv` row per (location, business date)
  over the same 400-day span, so `store_days.view.yml`'s join never drops or
  duplicates a trading day

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

# ── Store-day operations: the scenario/forecast fixture ─────────────────────
#
# Everything below builds `restaurant_store_days.csv` — one row per (location,
# business date), 24 x 400 = 9,600 rows. It exists for a different feature than
# the five signals above: the Metric Tree's **scenario simulation** (pin a
# lever, propagate a delta forward). That feature's behaviour is decided by the
# ARITHMETIC OPERATOR on each edge, and the check-grain tree is 100% additive —
# every edge in `checks.view.yml` is a `+`, so it can only ever exercise the
# "exact" propagation path. `store_days.view.yml` is where the other operators
# and the declared-driver cases live; see that file's header for the case-by-
# case map.
#
# Drawn from its own Random instance so the check/item stream above — and every
# statistic calibrated against it — is bit-for-bit unchanged by anything here.
STORE_DAY_SEED = 20260805

# ── Labor ──
# Hours = a fixed open/close block plus a sales-driven variable block. Tuned so
# labor lands near 30% of sales and sales-per-labor-hour near $60 — both inside
# the range casual dining actually runs at, which matters because
# `store_days.view.yml` exposes both as measures a scenario can pin.
LABOR_FIXED_HOURS = 8.0
LABOR_SALES_PER_VARIABLE_HOUR = 95.0
LABOR_HOURS_JITTER = 0.06
REGION_WAGE = {"west": 22.10, "northeast": 20.40, "midwest": 17.80, "south": 16.20}
# Wages drift up over the 400-day span, so `avg_wage` (a Div composite) is not
# a flat line and a scenario on it has something to move against.
WAGE_ANNUAL_DRIFT = 0.045

# ── Declared drivers ──
#
# A `drivers:` block asserts a marginal effect the engine will multiply a delta
# by. Rather than invent three coefficients, each driver series is CONSTRUCTED
# from the lagged outcome it is supposed to explain, so the coefficient is a
# real property of the data — and `check()` re-measures all three by
# within-location OLS and fails if any drifts more than DRIVER_SLOPE_TOL from
# what `store_days.view.yml` declares. The declared numbers are copied from
# that measurement, never estimated.
#
# Within-location is not a stylistic choice: base spend scales with a
# location's size, so an un-demeaned regression absorbs that contrast. Measured
# on this fixture: between-location 11.79, pooled 8.09, within-location 5.78 —
# so pooling overstates the marginal effect by 40%, and the pure between-store
# contrast (essentially 1 / MKT_SPEND_SHARE, the budget ratio) by 2x.
# Demeaning per location removes exactly that.
MKT_LAG_DAYS = 7
MKT_ROAS = 6.0  # incremental sales dollars per marketing dollar, 7 days later
MKT_SPEND_SHARE = 0.085  # base spend as a share of the location's mean daily sales
MKT_NOISE_RATIO = 0.20

PROMO_LAG_DAYS = 3
PROMO_COVERS_PER_REDEMPTION = 2.4
PROMO_BASE_PER_COVER = 0.62
PROMO_NOISE_RATIO = 0.20

LOYALTY_LAG_DAYS = 21
# A new loyalty member redeems more than one offer over the following three
# weeks, so this is above 1 by design.
LOYALTY_REDEMPTIONS_PER_SIGNUP = 2.5
LOYALTY_BASE_PER_REDEMPTION = 0.33
LOYALTY_NOISE_RATIO = 0.20

# `weather_severity_index` is the driver-side twin of `table_section`: declared
# on the view as a QUALITATIVE driver (direction/strength/confidence, no
# coefficient) and drawn independently of everything. It is the fixture's
# must-refuse case — see UNFITTABLE_MAX_T below, which is the guard that keeps
# it inert. A fixture where every driver can be sized cannot show a working
# refusal gate.

# ── The saturating driver ──
#
# Every driver above is LINEAR: the next dollar buys what the last one did.
# That is the easy case, and it is the wrong shape for most spend. This pair is
# deliberately curved — `delivery_orders = scale_loc * delivery_app_spend **
# DELIVERY_ELASTICITY` — and the view declares neither a `form:` nor a
# coefficient, so the engine INFERS the log-log shape from history and fits an
# ELASTICITY at query time rather than a slope.
#
# It exists to make a misread form visible. The same figure read as a level
# slope instead of an elasticity is out by a factor of `target / driver`, and a
# fixture where every edge is linear cannot tell a form-aware fit from one that
# ignores `form:` entirely — both return the same number on linear data. Here
# they cannot agree: the elasticity is ~0.45 and the level slope measured on the
# same rows is ~0.109 — 4.1x apart, which is the figure `--check` asserts and the
# one internal-docs/scenario-forecast.md quotes.
#
# Built FORWARD, unlike the three linear drivers above. Those are constructed
# from the outcome they explain, because that is what makes their slope a
# property of the data rather than a claim. Here the outcome is a new column
# with no other source, so spend is drawn first and orders follow from it —
# which is also the only direction that keeps the curve exact.
# Its own Random instance, for the reason STORE_DAY_SEED has one: drawing from
# the shared store-day stream shifts every draw after it, and the first attempt
# at this pair moved `marketing_spend -> net_sales` from 5.783 to 5.751 and with
# it every figure in the docstrings, the view and internal-docs/. Nothing
# calibrated against those streams may depend on whether this pair exists.
DELIVERY_SEED = 20260806
DELIVERY_ELASTICITY = 0.45
# Spread drawn log-UNIFORM, spanning a decade. A log fit needs variance in the
# logs, and the +-20% additive wobble the linear drivers use gives almost none:
# ln(1.2) - ln(0.8) is 0.4, against 2.3 here.
DELIVERY_SPEND_MIN = 40.0
DELIVERY_SPEND_MAX = 400.0
# Orders per spend**elasticity, drawn per location so panels sit at different
# levels and the within-location demeaning is doing real work — a pooled log
# regression is biased here for the same reason it is on marketing.
DELIVERY_SCALE_RANGE = (2.2, 5.0)
# Noise and the integer rounding of an order count both land on the OUTCOME, so
# they inflate the standard error without biasing the elasticity: in
# `ln y = ln k + b*ln x + ln(1+e)`, the noise term is absorbed by the intercept
# and then demeaned away. That is why the tolerance below can be tight.
DELIVERY_NOISE_RATIO = 0.06
# Days the delivery app was dark: spend 0, orders 0. `ln(0)` has no value, so
# these are exactly the rows a log fit must DROP and report (`n_nonpositive`)
# rather than silently narrow its window with. A fixture with no such day
# cannot test that path, and it is the commonest real one — every spend column
# has closed days in it. Kept well under the observation gate: 4% of 400 days
# is ~16 per location, leaving ~384 to fit on.
DELIVERY_DARK_DAY_RATE = 0.04

# ── The turning point ──
#
# Every driver above points ONE WAY for ever: linear buys the same per dollar,
# log-log buys a little less each time, but neither ever starts costing you. Real
# levers do. This pair is the fixture's inverted U —
#
#   promo_margin = level_loc + DISCOUNT_SLOPE*x + DISCOUNT_CURVATURE*x^2
#
# declares no `form:` and no coefficients, so the engine infers the quadratic
# shape and fits BOTH terms
# and can say where the lever stops paying.
#
# The story is the ordinary one: a shallow discount pulls in incremental guests
# whose margin more than covers the giveaway, so promo margin climbs. Push deeper
# and you start paying full-price guests to use a coupon they did not need, so the
# giveaway outruns the incremental volume and margin falls — eventually below
# where it started.
#
# The constants are SOLVED, not guessed, so the turn lands somewhere a scenario
# can actually reach. With x spread over DISCOUNT_MIN..DISCOUNT_MAX the peak sits
# at `vertex_x * s1/s2 - 1` as a proportional lever, so the vertex is placed to
# put that at +35% and break-even at +70% — both far inside the observed 6.6x
# spread, so the engine's domain backstop cannot swallow the case this exists to
# test. `check()` re-derives all three from the data on disk.
DISCOUNT_SEED = 20260807
DISCOUNT_MIN = 6.0  # dollars off per check, shallowest promo day
DISCOUNT_STEP = 1.4  # 25 steps to DISCOUNT_MAX
DISCOUNT_STEPS = 25
DISCOUNT_MAX = DISCOUNT_MIN + (DISCOUNT_STEPS - 1) * DISCOUNT_STEP  # 39.6
DISCOUNT_SLOPE = 0.85
# Vertex at $36.81 of depth: inside the observed range, so the curve genuinely
# turns over the data rather than being extrapolated past its own evidence.
DISCOUNT_VERTEX = 36.81
DISCOUNT_CURVATURE = -DISCOUNT_SLOPE / (2 * DISCOUNT_VERTEX)
# Per-location margin level, which the within-panel demeaning must remove — a
# pooled fit would read these level differences as part of the curve.
DISCOUNT_LEVEL_RANGE = (300.0, 900.0)
DISCOUNT_NOISE_SD = 3.0
# The curvature is the whole claim, so its own |t| is asserted, not just the
# slope's: a turning point resting on an insignificant x^2 term is a peak
# invented from noise, and the engine would report a ceiling that is not there.
#
# The floor sits well under the measurement (t ~ 33) and well over the engine's
# own |t| >= 2.0 bar, on the same principle as the t floors above: it asserts
# "unambiguously curved", not a value. Pinning it near the measured figure would
# make the fixture fail on harmless regeneration drift.
DISCOUNT_MIN_CURVATURE_T = 8.0

# The coefficients as WRITTEN in `store_days.view.yml`. They are the *measured*
# within-location slopes, not the construction constants above: the noise
# deliberately mixed into each driver series attenuates the recovered slope a
# few percent below the constant it was built from, and the YAML has to state
# what the data does, because the engine multiplies a pinned delta by whatever
# the YAML says. Update both sides together, from `--check`'s printed
# measurement — never from the construction constants.
DECLARED_COEFFICIENT = {
    "promo_redemptions -> guest_count": 2.30,
    "loyalty_signups -> promo_redemptions": 2.33,
    "guest_count -> net_sales": 52.0,
}
DRIVER_SLOPE_TOL = 0.10

# Edges that declare NO coefficient, where the engine measures one at query
# time instead (airlayer `metric_tree_fit`, within-location lagged OLS with a
# |t| >= 2.0 bar). What this file has to guarantee is not a specific slope but
# that each lands on the intended side of that bar — otherwise the two cases
# collapse into one and the refusal gate stops being tested.
#
# The measurement below is this file's own Python OLS, independent of the Rust
# implementation. The SLOPE is the cross-check between them: this file gets
# 5.783 and the engine gets 5.786 against the same fixture, which is what says
# the two implementations agree.
#
# Their t-statistics are NOT comparable and a maintainer should not read them
# as disagreeing. This file regresses spend against the 7-day SMOOTHED sales
# series it was constructed from (t ~ 486); the engine regresses against raw
# daily `net_sales`, because that is the measure the semantic layer exposes
# and it cannot know a smoothing existed. Smoothing strips most of the
# residual variance, so this file's standard error is optimistic by ~13x. The
# engine's t ~ 36 is the honest one.
FIT_MIN_T = 2.0
RUNTIME_FITTED = {
    # Must fit decisively. The floor is far below either measurement on
    # purpose: this asserts "unambiguously fittable", not a value, and it has
    # to hold for whichever of the two t's a future reader plugs in.
    "marketing_spend -> net_sales": 10.0,
    # Measured in LOGS, matching the log-log shape the engine infers.
    "delivery_app_spend -> delivery_orders": 20.0,
}

# What a log-log fit must recover: an ELASTICITY, not a slope. Unlike
# DECLARED_COEFFICIENT this is not copied into the YAML — the edge declares no
# coefficient precisely so the engine measures it — so this is the value the
# engine's own fit is checked against, in `verify_scenario_api.sh`.
#
# The tolerance is tight because the noise is on the outcome and so does not
# attenuate the slope (see DELIVERY_NOISE_RATIO). If a regeneration drifts it,
# the construction constant is what changed, not the estimator.
FITTED_ELASTICITY = {"delivery_app_spend -> delivery_orders": DELIVERY_ELASTICITY}
FITTED_ELASTICITY_TOL = 0.02
# The same pair fitted in LEVELS is what the engine would return if it ignored
# `form:`. That gap is the whole reason this pair is in the fixture, so it is
# asserted: measured 0.451 in logs against 0.109 in levels, 4.1x apart. The
# floor sits under that on purpose — like the t floors above, it asserts
# "unambiguously different", not a value. Raising it would mean pushing orders
# per dollar somewhere a delivery business does not sit, which buys nothing: a
# forecast 3.9x out is already unmistakable.
FORM_CONFUSION_MIN_RATIO = 3.0
# Must stay UNDER the engine's bar, with margin — a series that drifted to
# t = 1.9 would still pass the engine's gate today and fail it on the next
# regeneration, making the fixture's refusal case intermittent.
UNFITTABLE_MAX_T = 1.5

# ── The wound-down banquet program ──
#
# Private-event banquets ran until 120 days before the anchor and are exactly
# zero after. That gives the scenario simulation its `unquantifiable` case for
# free: `banquet_check_average = banquet_sales / banquet_covers` is a
# multiplicative edge, and a multiplicative edge whose CHILD is zero cannot be
# sized (%delta is undefined at zero), so on the default trailing-90-day window
# the engine must say "can't size this" rather than "no impact" — two very
# different claims. Widen the window to 365 days and the same edge sizes
# normally. Deliberately not the existing `catering` order channel, which is
# still live in the check data.
BANQUET_WIND_DOWN_DAYS = 120
BANQUET_COVER_PRICE = 38.0
BANQUET_COVER_WEIGHTS = ((0, 0.62), (12, 0.14), (18, 0.10), (24, 0.08), (40, 0.06))


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


def _mean(xs) -> float:
    xs = list(xs)
    return sum(xs) / len(xs) if xs else 0.0


def _stdev(xs) -> float:
    xs = list(xs)
    if len(xs) < 2:
        return 0.0
    m = _mean(xs)
    return math.sqrt(sum((x - m) ** 2 for x in xs) / (len(xs) - 1))


def _centered_mean(series: list[float], i: int, half_width: int) -> float:
    lo = max(0, i - half_width)
    hi = min(len(series), i + half_width + 1)
    return sum(series[lo:hi]) / (hi - lo)


def _lagged(series: list[float], i: int, lag: int) -> float:
    """`series[i + lag]`, clamped at the right edge.

    The last `lag` days have no future value to be constructed from. Clamping
    (rather than substituting the series mean) keeps the tail continuous
    instead of snapping to the base level; `check()` drops those rows from
    every driver regression, since they carry no real lead-lag information.
    """
    return series[min(i + lag, len(series) - 1)]


def _store_day_rows(checks: list[list], anchor: dt.date) -> list[list]:
    """One row per (location, business date) — the scenario/forecast fixture.

    Sales, covers and food cost are NOT stored here: `store_days.view.yml`
    re-derives them from the check data at query time, so the two grains can
    never disagree. What this table carries is the operational data that has no
    check-level source — labor, marketing, the loyalty program, weather, and
    the wound-down banquet business.

    Each of the three quantitative drivers is built backwards from the outcome
    it explains (spend from 7-day-ahead smoothed sales, redemptions from
    3-day-ahead covers, signups from 21-day-ahead redemptions), so the declared
    coefficient is a measurable property of the data rather than a claim. See
    the driver constants above.
    """
    rng = random.Random(STORE_DAY_SEED)
    # See DELIVERY_SEED: the saturating pair draws from its own stream so adding
    # it leaves every other column here bit-for-bit unchanged.
    drng = random.Random(DELIVERY_SEED)
    # Likewise its own stream: adding the turning-point pair must not shift any
    # column already on disk. See DISCOUNT_SEED.
    qrng = random.Random(DISCOUNT_SEED)
    start = anchor - dt.timedelta(days=DAYS - 1)
    dates = [(start + dt.timedelta(days=i)).isoformat() for i in range(DAYS)]
    date_index = {d: i for i, d in enumerate(dates)}
    region_of = {loc[0]: loc[2] for loc in LOCATIONS}
    banquet_last_day = (anchor - dt.timedelta(days=BANQUET_WIND_DOWN_DAYS)).isoformat()

    # Roll the generated checks up to (location, date). Indices follow the
    # `checks` row layout built in generate(): 1 location_id, 2 check_date,
    # 4 party_size, 5 total_amount.
    sales: dict[tuple[int, str], float] = defaultdict(float)
    covers: dict[tuple[int, str], float] = defaultdict(float)
    for c in checks:
        sales[(c[1], c[2])] += c[5]
        covers[(c[1], c[2])] += c[4]

    cover_levels, cover_weights = zip(*BANQUET_COVER_WEIGHTS)
    rows: list[list] = []
    for loc in LOCATIONS:
        loc_id = loc[0]
        day_sales = [sales[(loc_id, d)] for d in dates]
        day_covers = [covers[(loc_id, d)] for d in dates]
        # A 7-day centered mean is what marketing spend is set against: a
        # single store-day's sales swing is several times any plausible daily
        # ad budget, so building spend off the raw series would demand a
        # budget large enough to absorb it (or clip at zero constantly) and
        # destroy the very slope it is meant to carry.
        sma = [_centered_mean(day_sales, i, 3) for i in range(DAYS)]
        mean_sma = _mean(sma)
        mean_covers = _mean(day_covers)

        spend_signal = [(_lagged(sma, i, MKT_LAG_DAYS) - mean_sma) / MKT_ROAS for i in range(DAYS)]
        spend_base = MKT_SPEND_SHARE * _mean(day_sales)
        spend_noise_sd = MKT_NOISE_RATIO * _stdev(spend_signal)
        spend = [
            max(20.0, round(spend_base + spend_signal[i] + rng.gauss(0, spend_noise_sd), 2))
            for i in range(DAYS)
        ]

        promo_signal = [
            (_lagged(day_covers, i, PROMO_LAG_DAYS) - mean_covers) / PROMO_COVERS_PER_REDEMPTION
            for i in range(DAYS)
        ]
        promo_base = PROMO_BASE_PER_COVER * mean_covers
        promo_noise_sd = PROMO_NOISE_RATIO * _stdev(promo_signal)
        promo = [
            max(0, round(promo_base + promo_signal[i] + rng.gauss(0, promo_noise_sd)))
            for i in range(DAYS)
        ]

        mean_promo = _mean(promo)
        signup_signal = [
            (_lagged([float(p) for p in promo], i, LOYALTY_LAG_DAYS) - mean_promo)
            / LOYALTY_REDEMPTIONS_PER_SIGNUP
            for i in range(DAYS)
        ]
        signup_base = LOYALTY_BASE_PER_REDEMPTION * mean_promo
        signup_noise_sd = LOYALTY_NOISE_RATIO * _stdev(signup_signal)
        signups = [
            max(0, round(signup_base + signup_signal[i] + rng.gauss(0, signup_noise_sd)))
            for i in range(DAYS)
        ]

        # The saturating pair. Drawn forward: spend log-uniform over a decade,
        # orders as a power law of it. A dark day zeroes both — `ln(0)` is
        # undefined, so those are the rows the log fit has to drop and count.
        # On a live day the minimum order count is ~11, so a non-positive
        # `delivery_orders` can only ever be a dark day: `n_nonpositive` from
        # the engine should equal the dark-day count exactly.
        delivery_scale = drng.uniform(*DELIVERY_SCALE_RANGE)
        delivery_spend: list[float] = []
        delivery_orders: list[int] = []
        for _ in range(DAYS):
            if drng.random() < DELIVERY_DARK_DAY_RATE:
                delivery_spend.append(0.0)
                delivery_orders.append(0)
                continue
            spend_i = math.exp(
                drng.uniform(math.log(DELIVERY_SPEND_MIN), math.log(DELIVERY_SPEND_MAX))
            )
            orders_i = delivery_scale * spend_i**DELIVERY_ELASTICITY
            orders_i *= 1.0 + drng.gauss(0, DELIVERY_NOISE_RATIO)
            delivery_spend.append(round(spend_i, 2))
            delivery_orders.append(max(0, round(orders_i)))

        # The turning point. `x` sweeps the promo depth a manager might try;
        # `promo_margin` follows the inverted U that depth actually produces.
        # Built forward like the delivery pair, for the same reason: the outcome
        # is a new column with no other source.
        margin_level = qrng.uniform(*DISCOUNT_LEVEL_RANGE)
        discount_depth: list[float] = []
        promo_margin: list[float] = []
        for i in range(DAYS):
            x = DISCOUNT_MIN + (i % DISCOUNT_STEPS) * DISCOUNT_STEP
            m = margin_level + DISCOUNT_SLOPE * x + DISCOUNT_CURVATURE * x * x
            m += qrng.gauss(0, DISCOUNT_NOISE_SD)
            discount_depth.append(round(x, 2))
            promo_margin.append(round(m, 2))

        wage_base = REGION_WAGE[region_of[loc_id]] * rng.uniform(0.97, 1.03)
        for i, day in enumerate(dates):
            hours = (LABOR_FIXED_HOURS + day_sales[i] / LABOR_SALES_PER_VARIABLE_HOUR) * (
                1.0 + rng.uniform(-LABOR_HOURS_JITTER, LABOR_HOURS_JITTER)
            )
            wage = wage_base * (1.0 + WAGE_ANNUAL_DRIFT * i / 365.0)
            if day <= banquet_last_day:
                banquet_covers = rng.choices(cover_levels, weights=cover_weights, k=1)[0]
            else:
                banquet_covers = 0
            rows.append(
                [
                    loc_id,
                    day,
                    round(hours, 2),
                    round(hours * wage, 2),
                    spend[i],
                    promo[i],
                    signups[i],
                    round(rng.uniform(0.0, 1.0), 3),
                    banquet_covers,
                    round(banquet_covers * BANQUET_COVER_PRICE, 2),
                    delivery_spend[i],
                    delivery_orders[i],
                    discount_depth[i],
                    promo_margin[i],
                ]
            )

    # Emit in (date, location) order so the file reads as a diary rather than
    # 400 rows of location 1 followed by 400 of location 2.
    rows.sort(key=lambda r: (date_index[r[1]], r[0]))
    return rows


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
        "restaurant_store_days.csv": _store_day_rows(checks, anchor),
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
    "restaurant_store_days.csv": [
        "location_id",
        "business_date",
        "labor_hours",
        "labor_cost",
        "marketing_spend",
        "promo_redemptions",
        "loyalty_signups",
        "weather_severity_index",
        "banquet_covers",
        "banquet_sales",
        "delivery_app_spend",
        "delivery_orders",
        "discount_depth",
        "promo_margin",
    ],
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


def _ols_within(groups: dict[int, list[tuple[float, float]]]) -> tuple[float, float, float]:
    """Within-group (location fixed-effects) OLS slope of y on x, plus SE and t.

    Every point is demeaned against its own location before pooling, so the
    estimate is the marginal within-store effect. An un-demeaned regression
    absorbs between-store scale instead — a big store both sells more and
    budgets more. Measured here: pooled 8.09 against the true within-store
    5.78 (40% overstated), and the pure between-store contrast is 11.79, which
    is just the budget ratio wearing a ROAS label. Returns (slope, se, t).
    """
    xs: list[float] = []
    ys: list[float] = []
    for pts in groups.values():
        mx = _mean(p[0] for p in pts)
        my = _mean(p[1] for p in pts)
        xs.extend(p[0] - mx for p in pts)
        ys.extend(p[1] - my for p in pts)
    sxx = sum(x * x for x in xs)
    if not sxx:
        return float("nan"), float("inf"), 0.0
    slope = sum(x * y for x, y in zip(xs, ys)) / sxx
    dof = len(xs) - len(groups) - 1
    resid_ss = sum((y - slope * x) ** 2 for x, y in zip(xs, ys))
    se = math.sqrt((resid_ss / dof) / sxx) if dof > 0 else float("inf")
    return slope, se, (slope / se if se else float("inf"))


def _ols_within2(
    groups: dict[int, list[tuple[float, float, float]]],
) -> tuple[float, float, float, float]:
    """Within-group OLS of y on [x, x^2] — the two-term fit a turning point needs.

    The k=1 helper above cannot express a curve that turns, which is the whole
    reason `coefficients:` had to become a vector. This is the independent Python
    check on the engine's own two-term fit: it demeans both columns and the
    response against each location before pooling, exactly as the engine does, so
    a disagreement means the estimator changed and not the data.

    Returns (slope, curvature, t_slope, t_curvature).
    """
    xs: list[tuple[float, float]] = []
    ys: list[float] = []
    for pts in groups.values():
        n = len(pts)
        m1 = sum(p[0] for p in pts) / n
        m2 = sum(p[1] for p in pts) / n
        my = sum(p[2] for p in pts) / n
        for x1, x2, y in pts:
            xs.append((x1 - m1, x2 - m2))
            ys.append(y - my)
    n = len(xs)
    a11 = sum(x[0] * x[0] for x in xs)
    a12 = sum(x[0] * x[1] for x in xs)
    a22 = sum(x[1] * x[1] for x in xs)
    c1 = sum(x[0] * y for x, y in zip(xs, ys))
    c2 = sum(x[1] * y for x, y in zip(xs, ys))
    det = a11 * a22 - a12 * a12
    if not det:
        return float("nan"), float("nan"), 0.0, 0.0
    b1 = (a22 * c1 - a12 * c2) / det
    b2 = (a11 * c2 - a12 * c1) / det
    # One degree of freedom per panel mean AND per term, as the engine charges.
    dof = n - len(groups) - 2
    rss = sum((y - b1 * x[0] - b2 * x[1]) ** 2 for x, y in zip(xs, ys))
    s2 = rss / dof
    se1 = math.sqrt(s2 * a22 / det)
    se2 = math.sqrt(s2 * a11 / det)
    return b1, b2, (b1 / se1 if se1 else 0.0), (b2 / se2 if se2 else 0.0)


def _lagged_pairs(
    by_loc: dict[int, list[dict]],
    x_key,
    y_key,
    lag: int,
) -> dict[int, list[tuple[float, float]]]:
    """(x[i], y[i + lag]) pairs per location, dropping the unusable tail.

    The last `lag` rows of each location have no row `lag` days ahead to pair
    with — `_lagged` clamped them at generation time, so they carry no lead-lag
    information and would only dilute the slope. Dropped here rather than
    kept as zeros.
    """
    out: dict[int, list[tuple[float, float]]] = {}
    for loc_id, rows in by_loc.items():
        out[loc_id] = [(x_key(rows[i]), y_key(rows[i + lag])) for i in range(len(rows) - lag)]
    return out


def _check_store_days(failures: list[str]) -> None:
    """Verify the store-day fixture the scenario simulation runs on.

    Three things have to hold, and only the first is structural:

    1. The table lines up with the check data — one row per (location, date)
       over the same span, every location resolving.
    2. The unit economics are inside the band a restaurant actually runs at.
       `store_profit`, `prime_cost_pct` and `sales_per_labor_hour` are all
       measures a scenario can pin, and a lever on a measure whose baseline is
       nonsense produces a confidently nonsense forecast.
    3. Every coefficient declared in `store_days.view.yml` is what the data
       actually does, to within DRIVER_SLOPE_TOL. This is the one that rots
       silently: nothing in the engine checks a `drivers:` coefficient, so a
       declared 6.0 against data that says 3.0 would propagate wrong numbers
       forever with no error anywhere.
    """
    store_days = _read("restaurant_store_days.csv")
    checks = _read("restaurant_checks.csv")
    locations = {int(r["location_id"]): r for r in _read("restaurant_locations.csv")}
    menu = {int(r["menu_item_id"]): r for r in _read("restaurant_menu_items.csv")}
    items = _read("restaurant_check_items.csv")

    expected_rows = len(LOCATIONS) * DAYS
    if len(store_days) != expected_rows:
        failures.append(
            f"restaurant_store_days.csv has {len(store_days)} rows, expected "
            f"{expected_rows} ({len(LOCATIONS)} locations x {DAYS} days) — the view joins "
            f"on (location_id, business_date), so a missing pair silently drops a store-day"
        )
    orphan_locs = {int(r["location_id"]) for r in store_days} - set(locations)
    if orphan_locs:
        failures.append(f"{len(orphan_locs)} store_days rows reference a missing location")
    check_dates = {c["check_date"] for c in checks}
    missing_days = check_dates - {r["business_date"] for r in store_days}
    if missing_days:
        failures.append(
            f"{len(missing_days)} dates have checks but no store-day row (e.g. "
            f"{sorted(missing_days)[:3]}) — labor and marketing would read as zero there"
        )

    # ── Unit economics ──
    net_sales = sum(float(c["total_amount"]) for c in checks)
    food_cost = sum(
        int(i["quantity"]) * float(menu[int(i["menu_item_id"])]["unit_cost"]) for i in items
    )
    labor_cost = sum(float(r["labor_cost"]) for r in store_days)
    labor_hours = sum(float(r["labor_hours"]) for r in store_days)
    marketing = sum(float(r["marketing_spend"]) for r in store_days)
    labor_pct = labor_cost / net_sales
    prime_pct = (labor_cost + food_cost) / net_sales
    splh = net_sales / labor_hours
    print("\n  store-day unit economics (whole fixture):")
    print(f"    labor        {labor_pct:>7.1%} of net sales   (${labor_cost:,.0f})")
    print(f"    food         {food_cost / net_sales:>7.1%} of net sales   (${food_cost:,.0f})")
    print(f"    prime cost   {prime_pct:>7.1%} of net sales")
    print(f"    marketing    {marketing / net_sales:>7.1%} of net sales   (${marketing:,.0f})")
    print(f"    sales per labor hour  ${splh:,.2f}")
    for label, value, lo, hi in (
        ("labor as a share of sales", labor_pct, 0.26, 0.34),
        ("prime cost as a share of sales", prime_pct, 0.55, 0.68),
        ("sales per labor hour", splh, 50.0, 75.0),
    ):
        if not lo <= value <= hi:
            failures.append(
                f"{label} is {value:.3f}, outside the plausible band {lo}..{hi} — a scenario "
                f"pinned on a measure with an implausible baseline forecasts confident nonsense"
            )

    # ── Declared driver coefficients ──
    by_loc: dict[int, list[dict]] = defaultdict(list)
    for r in sorted(store_days, key=lambda r: (int(r["location_id"]), r["business_date"])):
        by_loc[int(r["location_id"])].append(r)
    day_sales: dict[tuple[int, str], float] = defaultdict(float)
    day_covers: dict[tuple[int, str], float] = defaultdict(float)
    for c in checks:
        key = (int(c["location_id"]), c["check_date"])
        day_sales[key] += float(c["total_amount"])
        day_covers[key] += int(c["party_size"])
    for loc_id, rows in by_loc.items():
        sales_series = [day_sales[(loc_id, r["business_date"])] for r in rows]
        for i, r in enumerate(rows):
            r["_sales"] = day_sales[(loc_id, r["business_date"])]
            r["_covers"] = day_covers[(loc_id, r["business_date"])]
            r["_sales_sma7"] = _centered_mean(sales_series, i, 3)

    marketing_fit = _ols_within(
        _lagged_pairs(
            by_loc,
            lambda r: float(r["marketing_spend"]),
            lambda r: r["_sales_sma7"],
            MKT_LAG_DAYS,
        )
    )
    measured = {
        "promo_redemptions -> guest_count": _ols_within(
            _lagged_pairs(
                by_loc,
                lambda r: float(r["promo_redemptions"]),
                lambda r: r["_covers"],
                PROMO_LAG_DAYS,
            )
        ),
        "loyalty_signups -> promo_redemptions": _ols_within(
            _lagged_pairs(
                by_loc,
                lambda r: float(r["loyalty_signups"]),
                lambda r: float(r["promo_redemptions"]),
                LOYALTY_LAG_DAYS,
            )
        ),
        "guest_count -> net_sales": _ols_within(
            _lagged_pairs(by_loc, lambda r: r["_covers"], lambda r: r["_sales"], 0)
        ),
    }
    print("\n  declared driver coefficients (within-location OLS):")
    for label, (slope, se, _t) in measured.items():
        declared = DECLARED_COEFFICIENT[label]
        drift = abs(slope - declared) / declared
        print(f"    {label:<38} measured {slope:>8.3f} (SE {se:.3f})  declared {declared:>7.3f}")
        if drift > DRIVER_SLOPE_TOL:
            failures.append(
                f"driver {label} measures a slope of {slope:.3f} but store_days.view.yml "
                f"declares {declared:.3f} ({drift:.0%} off, tolerance {DRIVER_SLOPE_TOL:.0%}) — "
                f"the engine multiplies a pinned delta by the DECLARED number, so a scenario "
                f"would forecast a move this data does not support"
            )

    # ── The three edges that declare no coefficient ──
    #
    # All three are handed to the engine's runtime fitter over the same window.
    # Two of them exist as a pair because they must land on OPPOSITE sides of
    # its |t| >= 2.0 bar — verifying only the one that fits would leave a
    # fitter that never refuses looking correct. The third is measured in a
    # different SPACE, which is what keeps a form-blind fit from passing.
    print("\n  runtime-fitted drivers (no coefficient declared):")
    mkt_slope, mkt_se, mkt_t = marketing_fit
    floor = RUNTIME_FITTED["marketing_spend -> net_sales"]
    print(
        f"    {'marketing_spend -> net_sales':<38} measured {mkt_slope:>8.3f} "
        f"(SE {mkt_se:.3f})  t {mkt_t:>7.2f}  must fit (t >= {floor})"
    )
    if abs(mkt_t) < floor:
        failures.append(
            f"marketing_spend -> net_sales measures t={mkt_t:.2f}, under the {floor} floor "
            f"this fixture needs — store_days.view.yml declares no coefficient for it on "
            f"purpose, so if the engine's fit stops clearing its |t| >= {FIT_MIN_T} bar the "
            f"edge silently goes inert and the runtime-fitting case stops being demonstrated"
        )

    # ── The saturating edge, measured in its declared space ──
    #
    # Two fits on the same rows. In LOGS it must recover the construction
    # elasticity; in LEVELS it returns something far away. The gap is the point:
    # a form-blind fitter passes every other assertion in this file, because
    # every other edge here is linear and both spaces agree there.
    delivery_pairs = _lagged_pairs(
        by_loc,
        lambda r: float(r["delivery_app_spend"]),
        lambda r: float(r["delivery_orders"]),
        0,
    )
    log_pairs = {
        loc: [(math.log(x), math.log(y)) for x, y in pts if x > 0 and y > 0]
        for loc, pts in delivery_pairs.items()
    }
    dark_days = sum(1 for pts in delivery_pairs.values() for x, _ in pts if x <= 0)
    elasticity, elast_se, elast_t = _ols_within(log_pairs)
    level_slope, _, _ = _ols_within(delivery_pairs)
    expected = FITTED_ELASTICITY["delivery_app_spend -> delivery_orders"]
    floor = RUNTIME_FITTED["delivery_app_spend -> delivery_orders"]
    print(
        f"    {'delivery_app_spend -> delivery_orders':<38} measured {elasticity:>8.3f} "
        f"(SE {elast_se:.3f})  t {elast_t:>7.2f}  elasticity, log-log (expect {expected})"
    )
    print(
        f"    {'  ... the same rows fitted in levels':<38} measured {level_slope:>8.3f}"
        f"{'':>28}  <- what a form-blind fit returns"
    )
    print(
        f"    {'  ... dark days (spend 0, no log)':<38} {dark_days:>8} rows the log fit "
        f"must drop and report"
    )
    if abs(elasticity - expected) > FITTED_ELASTICITY_TOL:
        failures.append(
            f"delivery_app_spend -> delivery_orders measures an elasticity of "
            f"{elasticity:.3f}, expected {expected} +-{FITTED_ELASTICITY_TOL} — the engine fits "
            f"this edge at query time and verify_scenario_api.sh checks its answer against "
            f"{expected}, so the two would disagree"
        )
    if abs(elast_t) < floor:
        failures.append(
            f"delivery_app_spend -> delivery_orders measures t={elast_t:.2f} in logs, under the "
            f"{floor} floor — this is the fixture's only non-linear fittable edge, and a refusal "
            f"here leaves the log-log path untested"
        )
    if not dark_days:
        failures.append(
            "no dark delivery days (spend 0) — those rows are the fixture's only test that a log "
            "fit DROPS a non-positive value and reports how many, instead of silently narrowing "
            "its window"
        )
    ratio = abs(elasticity / level_slope) if level_slope else float("inf")
    if ratio < FORM_CONFUSION_MIN_RATIO:
        failures.append(
            f"the log-log elasticity ({elasticity:.3f}) and the level slope ({level_slope:.3f}) "
            f"are only {ratio:.1f}x apart, under the {FORM_CONFUSION_MIN_RATIO}x this fixture "
            f"needs — they exist to be far enough apart that a fit ignoring `form:` cannot pass "
            f"for a fit honouring it"
        )

    # ── The turning point, and where it turns ──
    #
    # The only shape in this project that can stop helping. Three things are
    # asserted, because each guards a different failure:
    #   1. both coefficients come back — a one-term fit cannot describe a curve
    #      that turns, and would report a lever that pays for ever;
    #   2. the CURVATURE is decisively significant — a peak resting on an
    #      insignificant x^2 term is invented from noise, and the engine would
    #      show a ceiling that is not there;
    #   3. the peak and break-even land inside the observed spread, so the
    #      engine's domain backstop cannot swallow the case.
    disc_groups = {
        loc: [
            (float(r["discount_depth"]), float(r["discount_depth"]) ** 2, float(r["promo_margin"]))
            for r in rows
        ]
        for loc, rows in by_loc.items()
    }
    d_slope, d_curve, d_t1, d_t2 = _ols_within2(disc_groups)
    depths = [float(r["discount_depth"]) for r in store_days]
    s1 = sum(depths)
    s2 = sum(x * x for x in depths)
    # dY(r) = slope*s1*r + curvature*s2*((1+r)^2 - 1); solve dY' = 0 and dY = 0.
    peak_r = -(d_slope * s1) / (2 * d_curve * s2) - 1
    zero_r = -(d_slope * s1) / (d_curve * s2) - 2
    spread = max(depths) / min(depths)
    print("\n  the turning point (quadratic shape inferred, nothing declared):")
    print(
        f"    {'discount_depth -> promo_margin':<38} slope {d_slope:>8.4f} (t {d_t1:>7.1f})  "
        f"curvature {d_curve:>9.6f} (t {d_t2:>7.1f})"
    )
    print(
        f"    {'  ... peaks at':<38} +{peak_r * 100:>7.1f}%   break-even +{zero_r * 100:.1f}%   "
        f"observed spread {spread:.1f}x"
    )
    if abs(d_slope - DISCOUNT_SLOPE) / DISCOUNT_SLOPE > DRIVER_SLOPE_TOL:
        failures.append(
            f"discount_depth -> promo_margin measures a slope of {d_slope:.4f}, built from "
            f"{DISCOUNT_SLOPE} — the engine fits this edge at query time and "
            f"verify_scenario_api.sh checks its answer against the construction constant"
        )
    if abs(d_curve - DISCOUNT_CURVATURE) / abs(DISCOUNT_CURVATURE) > DRIVER_SLOPE_TOL:
        failures.append(
            f"discount_depth -> promo_margin measures a curvature of {d_curve:.6f}, built from "
            f"{DISCOUNT_CURVATURE:.6f} — the curvature IS the turning point, so a drift here "
            f"moves where the fixture says the lever stops paying"
        )
    if abs(d_t2) < DISCOUNT_MIN_CURVATURE_T:
        failures.append(
            f"the curvature term measures t={d_t2:.2f}, under the {DISCOUNT_MIN_CURVATURE_T} "
            f"floor this fixture needs. The engine requires EVERY basis term to clear |t| >= "
            f"{FIT_MIN_T}, so at this t it would refuse the edge and the turning-point case "
            f"would silently stop being tested"
        )
    if not 0.0 < peak_r < zero_r < spread - 1:
        failures.append(
            f"the turn is not reachable: peak at {peak_r:.2f}, break-even at {zero_r:.2f}, "
            f"observed spread {spread:.2f}x. Both have to be positive, ordered, and inside the "
            f"spread — outside it the engine's domain backstop refuses the lever, which is the "
            f"right behaviour but leaves 'helps, then hurts' untested"
        )

    weather_slope, weather_se, weather_t = _ols_within(
        _lagged_pairs(by_loc, lambda r: float(r["weather_severity_index"]), lambda r: r["_covers"], 0)
    )
    print(
        f"    {'weather_severity_index -> guest_count':<38} measured {weather_slope:>8.3f} "
        f"(SE {weather_se:.3f})  t {weather_t:>7.2f}  must NOT fit (t < {UNFITTABLE_MAX_T})"
    )
    if abs(weather_t) >= UNFITTABLE_MAX_T:
        failures.append(
            f"weather_severity_index moves guest_count at t={weather_t:.2f}, at or above the "
            f"{UNFITTABLE_MAX_T} ceiling — this is the fixture's must-refuse case, and it needs "
            f"margin under the engine's |t| >= {FIT_MIN_T} bar. At t={weather_t:.2f} the engine "
            f"would start fitting a coefficient for a variable that does nothing, which is the "
            f"exact failure the refusal gate exists to prevent"
        )

    # ── The wound-down banquet program ──
    anchor = max(r["business_date"] for r in store_days)
    wind_down = (
        dt.date.fromisoformat(anchor) - dt.timedelta(days=BANQUET_WIND_DOWN_DAYS)
    ).isoformat()
    live = sum(float(r["banquet_sales"]) for r in store_days if r["business_date"] <= wind_down)
    dead = [r for r in store_days if r["business_date"] > wind_down]
    still_selling = [
        r for r in dead if float(r["banquet_sales"]) or int(r["banquet_covers"])
    ]
    print(
        f"\n  banquet program: ${live:,.0f} through {wind_down}, then exactly zero for "
        f"{len(dead)} store-days"
    )
    if live <= 0:
        failures.append("banquet_sales is zero even before the wind-down date — nothing to size")
    if still_selling:
        failures.append(
            f"{len(still_selling)} store-days after {wind_down} still carry banquet activity — "
            f"the zero-denominator case (banquet_sales / banquet_covers on a multiplicative "
            f"edge) needs the trailing window to be exactly zero"
        )
    # The default scenario period preset is a trailing 90 days. The wind-down
    # has to sit outside it, or the unquantifiable case never fires on the
    # default view and only shows up if someone widens the window by hand.
    if BANQUET_WIND_DOWN_DAYS <= 90:
        failures.append(
            f"BANQUET_WIND_DOWN_DAYS is {BANQUET_WIND_DOWN_DAYS}, inside the 90-day default "
            f"scenario window — banquet_covers would be non-zero there and the "
            f"unquantifiable case would stop firing on the default period"
        )


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

    _check_store_days(failures)

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
