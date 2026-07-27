# `oxy-authz` — the authorization decision layer (`crates/authz`)

**One place states who may do what.** Every authorization decision in Oxy resolves to one
arm of a single `match` in `allows()`. A rule that is not stated there is not a rule.

Authentication (who you are) is `oxy-auth`. This is authorization (what you may do).

- **Why it exists:** authentication was a layer; authorization was a *scatter*. Once the
  extractor handed a handler a `User`, ~170 call sites decided access ad hoc, and the
  copies drifted from their siblings. Writing the model down and differencing it against
  the shipped checks found five real bugs — see the PR body and the design doc.
- **Why there is no policy engine:** this ran on Cedar and the engine was removed. The
  crate header (`src/lib.rs`) records the reasoning; don't relitigate it from scratch.
  Short version: policy-as-data is an explicit non-goal (design §2), and that is the
  requirement that pays for an engine. Adopt one when policy must be authored by someone
  outside this repo — not to compute `contains`.
- **Design:** the model + rejected-engines rationale are captured in this guide and
  `src/lib.rs`; the original unification design doc's history is in git.

## The one boundary that matters: model vs facts

| | Lives in | Says |
| --- | --- | --- |
| **The model** | `oxy-authz` (this crate) | what a principal is *entitled to* |
| **The facts** | `oxy-app` (`server::authz::loader`) | what is *true of* the principal |

The loader stays in `oxy-app` because it reaches app primitives — org membership, partner
standings, the `app_admins` table. This crate depends on `uuid` + `tracing` and nothing
else, which is what lets the whole model be tested without a database.

**Don't add a DB, HTTP, or `entity` dependency here.** A rule that needs a query is a sign
the fact is missing from `PrincipalFacts`, not that the crate needs a connection.

## Vocabulary

- **`Action`** (23) — the closed vocabulary of things a caller can do. This is what call
  sites name.
- **`Ring`** (13) — the authority level that gates an action. **Private on purpose:** a
  ring is how the model is *stated*, not a menu callers pick from. Public, it would let a
  call site choose its own authority level — the scatter this crate exists to end.
- **`PrincipalFacts`** — the whole input surface. Empty = denied everything (fail closed).
- **`Resource`** — what's being acted on: `org_id`, `kind`, optional `owner`, optional
  acting `partner`.
- **`Cap`** (8) — a partner ceiling capability, one-to-one with `PartnerCapability`.

Rings, briefly: `Read` · `MemberStrict` · `OrgAdmin` · `OrgAdminStrict` · `OwnerOnly` ·
`OrgAdminOrCreator` · `WorkspaceAdmin` · `WorkspaceAdminStrict` · `WorkspaceEdit` ·
`AppAccess` · `PartnerCap` · `GlobalAdminOrOwner` · `GlobalOwnerOnly`.

The `*Strict` variants reject the global-operator override. That distinction is
load-bearing and has already been got wrong once (billing was modeled `OwnerOnly`; the
real gate is a *real* owner/admin with the override rejected).

## Entry points — pick the right one

| Call | Decision | Use when |
| --- | --- | --- |
| `enforce(label, facts, action, resource, existing_allow)` | `existing_allow && allows(..)` | **The default.** A shipped check exists to difference against. |
| `require(facts, action, resource)` | `allows(..)` | No legacy check exists (a new surface), or its legacy term was retired. |
| `authorize(..)` | `allows(..)`, legacy observed only | **Currently unwired, deliberately.** Drops the fail-safe. |

From `oxy-app`, prefer the wrappers in `server::authz`: `enforce_guard` (in a guard,
memoized facts), `enforce_for` (a call site holding a DB handle + identity),
`partner_allows` (the partner tier).

### `existing_allow` is the oracle, not ceremony

The conjunction is the whole safety property: the model can only ever **subtract** access
the existing check granted, so a mis-modeled ring **cannot open a hole**. The residual
failure is a wrong *deny* — loud (a 403), attributable (a WARN naming the label),
revertible in one line.

Passing a hand-waved `true` silently converts a fail-safe into a bare `allows` **and**
throws away the oracle the differential tests difference against. If there's genuinely no
existing check, use `require` and say so.

## Adding an `Action`

1. Add the variant + a doc comment saying **who** may do it and **why** — including who
   deliberately may *not* (the override, a partner, staff).
2. Add it to `Action::ALL` and give `as_str` a stable id. That id is a **wire contract** —
   it lands in the `authz` tracing output.
3. Map it to a `Ring` in `ring()`. Skipping this fails the build; that exhaustiveness is
   the point of the enum.
4. Add a case to `server::authz::differential` asserting the ring matches the shipped
   check across every caller shape. Reuse an existing ring rather than inventing one —
   two rings that mean the same thing is how drift restarts.

## Testing

Validation here is **differential, not a shadow window**. At low traffic `disagree == 0`
just means nobody hit it — a false green. The gate is differencing the model against the
legacy oracle, which is what caught every real bug.

| Suite | Proves |
| --- | --- |
| `crates/authz` unit tests (18) | the model's own arithmetic |
| `server::authz::differential` | the ring agrees with the shipped guard across the caller-shape space |
| `crates/app/tests/authz_loader_differential.rs` | the **real loader** against seeded rows (needs `OXY_DATABASE_URL`; skips without) |
| `crates/app/tests/authz_boundaries.rs` | nothing outside the allowlist decides access by hand |

The unit differential hand-builds facts, so it tests an *assumption* about the loader; the
seeded suite is what tests the loader. Don't drop a fail-safe on the strength of the
former alone.

## Pitfalls

- **Operator flags are not unconditional.** Global standing must not out-rank a *real*
  membership — an Oxy staffer who is a plain member of a tenant is a plain member there.
  The operator terms are gated on non-membership.
- **`develop_apps` is not `manage_apps`.** Data-plane access vs app lifecycle. Conflating
  them hands a partner another tenant's data.
- **Self rules must be scoped to kind AND action**, or the owner of any future
  owner-bearing resource inherits every action on it.
- **Capability must come from the partner being acted as.** Holding a capability through
  partner B must not authorize anything while scoped to A. `PartnerStanding` keeps the
  partner rather than flattening the sets, which is exactly what makes this expressible.
- **Facts load at most once per request** (memoized in request extensions). On a hot path
  use `load_principal_facts_scoped` so a ring doesn't pay for facts it never reads.
