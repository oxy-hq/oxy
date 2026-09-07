# Declared worlds

Each file here is one point on the grid — a world whose true parameters we chose
and never told the model. They are files rather than CLI flags because the
question is *where* the algorithm works, not whether it works somewhere, and a
grid you cannot diff or review is not evidence.

Naming is `<axis>.simulation.yml`. The axes and what each bites are in
[`internal-docs/2026-08-12-simulation-in-oxygen-plan.md`](../internal-docs/2026-08-12-simulation-in-oxygen-plan.md#the-worlds-to-declare).

`clean` (rho 0) and `confounded` (rho 0.7) were the confounding axis's only two
points until `moderate_confounding` (rho 0.35, half the shock size too) filled in
the row's middle — bias 1.070 under `oxy_simulation::check`, against `clean`'s 1.026
and `confounded`'s 1.066 on the same seed, so halving the shock knobs did not halve the bias.

## A world is not an experiment

**No file here declares a `policy:`.** The arms — `hold`, `legacy`, `machine`,
`machine_explore`, `oracle` — are chosen when a run is queued:

```
POST /api/{workspace}/simulations/confounded/runs?policies=hold,machine
```

A world is what happens; a policy is what someone does about it. There used to be
four `confounded_*` files differing by one line, and they were four worlds that
happened to look alike: the profit race is only attributable because every arm
sees the same seed, the same shocks and the same noise, and nothing enforced that
when a reviewer could edit one file and not the other three. `deny_unknown_fields`
means a leftover `policy:` line is now a loud spec error rather than a silently
ignored one.

The race enforces the rest of it. Each run stores its own **spec snapshot**, and
`GET /simulations/{name}/race` pairs arms on a fingerprint of that snapshot — not
on the seed, and not on the replicate number. Editing this file between queueing
two arms therefore withholds the comparison as `disjoint_worlds` rather than
reporting the edit as a policy effect: the seeds still match after an edit to
`entities.count` or `noise_ratio`, so a seed-keyed race would have paired two
different worlds, come back with a large margin and a confident p-value, and had
nothing in the response to say why.

The same reasoning puts **`replicates:`** in the file and not on the run: how many
draws it takes before a cell of the outcome map can be called is a property of how
noisy the world is. `noisy`, `few_panels` and `flat_lever` declare 5 because one
seed there classifies the draw rather than the world — `few_panels` is the corner
where the independent estimator returns a *sign-flipped* −0.84. Replicate 0 always
runs the declared seed; later draws walk it by one.

**These deliberately include the worlds where the machine loses.** A demo built
to avoid finding those is worth nothing: the first buyer who asks "my budget
tracks my revenue — what then?" breaks it.

Two lags, deliberately. `lag_days` is what the world generates against;
`declared_lag_days` is what the generated `.view.yml` claims, and therefore what
the fitter pairs on. Omit the second and the customer guessed right. One field
could not carry both roles — reading the true lag into the view made the customer
right by construction and put the whole lag-error axis out of reach.

Two constants to leave alone unless you have re-derived them:

- `anchor_spend_share: 0.02` with `local_slope_at_anchor: 4.0` puts marketing at
  ~12% of revenue and settles the world ~1.27x above its anchor. The plan's
  original 8.5% / 6.0 is not physically coherent forward — it puts marketing's
  *total* contribution above 100% of base sales.
- `optimum_at` must exceed `margin x local_slope_at_anchor` or no saturating
  curve can place it; the spec refuses with the floor named.

## The noise ceiling

`net_sales` is floored at zero, so a world noisy enough to hit that floor stops
emitting the curve it declares — the clamp is a second, undeclared mechanism.
Measured boundary: drift appears somewhere between `noise_ratio` 0.45 and 0.5 at
`sales_per_entity_day: 1500` with `scale_sigma: 0.4`. `noisy` sits at 0.30 for
margin.

The self-check is what catches this (`every_declared_world_emits_the_mechanism_it_declares`),
and it is the reason that test exists: a clamped world still produces a run that
looks fine, and every convergence claim from it would be scored against a curve
the world does not contain.

## The flat-lever floor

`budget_jitter_sd` is the identification axis — the only movement in the regressor
that is *not* the confounder — and `flat_lever` turns it down to 0.005. It cannot
be turned off, and the reason is worth knowing before reading that world's result.

Measured on the `confounded` constants (within-panel CV of spend, which is what the
fit actually sees — the pooled figure is dominated by entity size and says nothing):

> `oxy_simulation::readiness` is a library the fitter does not yet call — no run
> screens the declared worlds by these thresholds. What is implemented and
> unit-tested there is the rule: demean by panel before correlating spend against
> trailing sales, and measure identifying variation as
> `sqrt(Σ (x − x̄_p)²) / sqrt(mean(x²))` rather than a between/within ratio. The
> figures below come from running that measurement over the simulation grid, not
> from a gate any run currently applies. The argument for the rule still holds: a
> ratio of two dispersions is not a precision, and gating on one ranked these
> worlds backwards — it fired hardest at wide `scale_sigma`, which is where `se`
> is *tightest*.

| what is left moving the budget | within-panel CV |
| --- | --- |
| jitter 0.12, shock on | 0.164 |
| jitter 0.005, shock on | 0.114 |
| jitter 0.005, no shock | 0.046 |
| jitter 0.005, nothing else random | 0.041 |
| **jitter 0.0, nothing else random** | **0.040** |

The floor is the burn-in climbing from its anchor to the budget rule's fixed point
— spend raises sales, which raises the budget — a deterministic ramp inside every
panel. So a legacy budget rule cannot produce a still lever, and what it leaves is
the worst kind of variation: perfectly correlated with the mechanism being
measured. The left column of the map is approached, never reached.
(`the_budget_jitter_knob_moves_how_much_the_lever_varies` pins every row of that
table.)

`wide_lever` is this axis's other end: same confounding as `confounded`, but
`budget_jitter_sd` turned up to 0.30 against the 0.12 default instead of down.
Bias there is 1.055 under the same check, against `confounded`'s 1.066 at the default jitter
and `flat_lever`'s 0.884 at the floor — going wide barely moves the number, so
`budget_jitter_sd` reads as an identification knob, how often the gate can even
speak, more than a bias one.
