---
name: oxy-semantic-modeling
description: Use when writing, reviewing, or debugging an Oxy semantic model — `.view.yml` views, `.topic.yml` topics, measures, dimensions, entities, and the `meta:` freshness block — in a customer workspace, and when one misbehaves: a measure that vanishes after switching topics, "measure not found", a topic returning no rows or missing joins, or an agent reporting a partial day as a complete number. Carries the craft and the traps the reference docs do not.
---

# Oxy semantic modeling

**The reference is upstream and maintained — read it rather than guessing:**
<https://www.oxygen-hq.com/docs/guide/build/semantic-model/simple-model>
(the section entry point; `views`, `entities`, `dimensions`, `measures`,
`topics`, `modeling`, and `use-in-agents` sit beside it).

That page owns the field lists. This skill deliberately does **not** restate
them — a copied schema stays confidently wrong long after the real one moves.
What follows is the craft and the traps it does not carry, with every example
taken from Poke House's real 39 views and 23 topics.

## View anatomy

A view is one physical table plus the vocabulary an agent is allowed to use
against it.

```yaml
name: sales_daily
datasource: "clickhouse"
table: "restaurant_analytics.restaurant_analytics___sales_daily_metrics"
```

**`entities` are the join graph, not decoration.** The `primary` entity names
the grain of the row; `foreign` entities are the edges to other views; `parent:`
states hierarchy (a sales day belongs to a restaurant).

```yaml
entities:
  - name: sales_day
    type: primary
    key: sales_day_key
    parent: restaurant
  - name: restaurant
    type: foreign
    key: restaurant_id
```

**`key` names a dimension, not a column.** In `restaurants.view.yml` the entity
`restaurant` has `key: restaurant_id`, and the `restaurant_id` *dimension* is
`expr: guid` — the physical column is `guid`. So resolve a `key` against the
view's `dimensions:` list, never against the table's columns; the reference's
`entities` page carries the same warning.

**`dimensions`** are what you slice by, **`measures`** what you aggregate. Both
take `synonyms`; measures also take `filters` and `drivers`. House style keeps
entity keys single — even a compound natural key is one pre-concatenated
dimension (`sales_day_key`); no view uses the documented composite `keys:`.

## The description is a contract with the agent, not a label

`sales_daily`'s description, verbatim:

> Daily sales metrics aggregated by restaurant and business date. Loaded several
> times daily; the current business day is always intraday-partial - the prior
> full day is the latest complete day.

The second sentence is the whole point. Without it an agent asked "how are sales
today?" reports a half-loaded partial day as a real number, confidently and with
no warning — wrong in a way nobody can see downstream. Write descriptions that
say when the data is complete and what it excludes, not what the table holds.

## The `meta:` freshness block

Four keys — `freshness_watermark_column`, `freshness_expected_cadence`,
`freshness_complete_through`, `freshness_caveat` — that state a view's own lag
characteristics so the agent can qualify an answer instead of guessing.

**Only 11 of 39 views carry one.** That low number is the point: it is the most
under-used high-value feature in a real model, and adding one is the cheapest
quality win available on any view fed by a batch pipeline.

- **It is undocumented.** Not one of the four keys appears anywhere in the docs,
  and `views.mdx`'s own property table does not list `meta`. It is a working
  project convention, not a schema feature you can look up. **Copy the block from
  `sales_daily.view.yml`** — that is the canonical exemplar.
- **Every value is wrapped as a single-element list** (`["prior full day
  (America/Los_Angeles)"]`) even though each reads as a scalar. "Simplifying" it
  to a bare string silently deviates from all 11 existing blocks.

## Craft

**`synonyms` are the natural-language surface.** Map both domain jargon and plain
English onto one name; colliding common words across views is normal and wanted.

```yaml
  - name: total_net_sales
    synonyms: ["net sales", "net revenue", "revenue", "sales"]
  - name: sales_per_guest
    synonyms: ["per guest average", "guest average", "PPA"]
```

**`samples` on enum-ish dimensions** tell the agent the valid values, so it
filters on `"PAID"` instead of inventing `"paid"`. Give the *full* set, not a
sample — `["OPEN", "PAID", "CLOSED"]`, `["negative", "neutral", "positive"]`.

**Derived time dimensions so the agent never writes date arithmetic.** Ship
`business_year`, `business_month`, `business_quarter`, `day_of_week`,
`is_weekend` as dimensions of their own, each re-derived from the raw column
rather than referencing the base date dimension:

```yaml
  - name: business_quarter
    expr: "toQuarter(toDate(parseDateTimeBestEffort(toString(business_date))))"
```

**`drivers` must state a mechanism, not a vibe.** The quality bar, from
`sales_daily`:

```yaml
  - name: total_net_sales
    drivers:
      - measure: sales_daily.discount_rate
        direction: negative
        strength: strong
        confidence: high
        description: "A larger share of gross sales given away as discounts lowers net sales. Unlike discount dollars, the rate does not move with volume."
```

Note what the `description` does: it names the causal path *and* says why the
obvious alternative (discount dollars) was rejected. The weak examples in the
corpus — in `quickbooks_pl.view.yml` — carry `direction`, `strength`,
`confidence`, and a `lag`, but no `description` at all, so the mechanism is never
stated and a reader cannot tell whether the edge was reasoned or guessed. A
driver without a `description` is not finished. Only 8 of 39 views use
`drivers`, and the field is absent from the semantic-model docs — see the traps.

## Topics are question areas, not tables

A topic is a `base_view` plus the views it may join to, and the filters that
should always apply. That is the whole shape — all 23 real topics use only
`name`, `description`, `views`, `base_view`, `default_filters`.

```yaml
name: orders
base_view: orders
views:
  - orders
  - order_checks
  - order_selections
  - restaurants
  - dining_options
default_filters:
  - field: "is_voided"
    eq:
      value: false
```

Name topics after the questions people ask, and let `default_filters` carry the
"obviously we don't count voided orders" rules so no one has to remember them.

## Traps

**The `airhouse_*` twins are a migration artifact, and they are not mirrors.**
The ClickHouse → Airhouse migration is in flight, so **11 of the 39 views exist
twice** (22 views, 11 twin pairs) — plus 3 further `airhouse_*` views with no
ClickHouse counterpart at all, where discount- and payment-level detail lives
only on the new side. Three consequences, in order of how much time they cost:

1. **The twins have diverged.** `sales_daily` defines a `discount_rate` measure;
   `airhouse_sales_daily` does not. Switch topics mid-conversation from one
   family to the other and a query that worked seconds ago fails with "measure
   not found", with nothing anywhere saying the two views were ever supposed to
   match. Before assuming a measure exists on a twin, read the twin.
2. **The two families cannot join to each other, and fail silently when you
   try.** Every `airhouse_*` view prefixes its **foreign** entity names —
   the edges — as `airhouse_restaurant`, `airhouse_order`, `airhouse_check`,
   while its twin uses bare `restaurant`, `order`, `check`. Joins resolve by
   matching entity *names*, so a topic mixing families does not error — it just
   fails to auto-join. You get missing rows, not a config failure. No real topic
   mixes them; keep it that way.

   **The prefix is not universal, and the exception runs the other way.** The
   three twin-less views from above leave their **primary** entity bare:
   `airhouse_order_discounts` → `discount_application`,
   `airhouse_order_payments` → `payment`, `airhouse_selection_discounts` →
   `selection_discount_application`. Their foreign entities are still prefixed,
   so rule 2 holds — but the latent failure inverts. A bare primary is exactly
   what a ClickHouse-family view would declare, so if anyone ever adds a
   `payment` or `discount_application` entity on the old side, those two views
   start auto-joining *across* families rather than failing to join: wrong rows
   silently, instead of missing rows silently. No such collision exists in the
   corpus today (grep: each of the three names is declared in exactly one
   view). Prefix the primary too on any new `airhouse_*` view.
3. **Do not copy the pattern.** Duplicating a view to change backends is a
   migration tactic with an end date, not a modelling technique.

**Repeated defensive casting is a smell.** `toDate(parseDateTimeBestEffort(
toString(business_date)))` appears in **6 derived dimensions of `sales_daily`** —
the same three-deep cast chain re-typed six times because the underlying column
is not a date. Fix the column type once, upstream in the pipeline, rather than in
every `expr`. (Counting these by grep over-reports: a seventh occurrence is
inside `measures:`, not `dimensions:`. The dimension count is 6.)

**`drivers` and `parent:` are documented, but not where you would look.** Neither
appears in the semantic-model pages — `drivers` is explained under
<https://www.oxygen-hq.com/docs/guide/build/world-model/metric-tree> and
`parent:` under <https://www.oxygen-hq.com/docs/guide/build/world-model>. Worse,
the metric-tree page links back to `measures` claiming it defines the measures
"and their drivers", which it does not. Read only the semantic-model section and
you will conclude both fields do not exist and model without them.

**View-qualified `{{ }}` references are the one-off, not the norm.** Across all
39 views the `{{ view.measure }}` form appears in exactly **one expression** —
the `discount_rate` measure at `sales_daily.view.yml:129`, which uses it twice
in that one line (`{{ sales_daily.total_discounts }} / NULLIF({{
sales_daily.total_gross_sales }}, 0)`). So a grep returns 2 hits from 1 site;
neither number is "the norm". Every other `{{ }}` in the corpus is a bare,
unqualified dimension name inside a measure's `filters:` — `{{is_voided}}`. For
an ordinary same-view filter, write the bare form; copying the qualified one
means copying the single outlier in the codebase.
