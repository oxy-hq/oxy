---
name: oxy-app-visual-identity
description: Use when rendering or sourcing a custom app's icon/favicon or screenshot/art on ANY surface — the homepage launcher card (web-app/src/pages/launcher), the admin apps browser (web-app/src/pages/admin/AdminCustomApps), an app rail tile, or any new place that shows an app's picture. Also when adding icon/art fields to an apps API DTO (workspace_custom_apps.rs, admin/apps/handlers.rs). Triggers on "app icon", "app favicon", "app art", "app screenshot", "AppCard icon", "AppFavicon", "icon_url", "art_url", "launcher card image", "app thumbnail/preview", "monogram fallback".
---

# Customer-app visual identity (icon + art), unified

A custom app has exactly **two pictures**, and there is **one way** to source and
render each — everywhere. Do not grow a second mechanism per surface.

**Scope: platform rendering only.** This is how *oxy* sources + renders app pictures.
How an app *author* declares `icon`/`art` and ships the asset files is a customer-apps
concern — see the `hello-oxy` example + the `customer-app-development` skill in the
`customer-apps` repo, not here. The field contract itself is defined once in
`OxyAppManifest` (+ the SDK manifest type); this skill only references it. Keep
authoring how-to out of here so the two don't drift.

## The single source of truth

Both pictures come **only** from the bundle's `oxy-app.json` manifest, as relative
paths inside the bundle:

| Manifest field | Meaning | API field | Rendered as |
| --- | --- | --- | --- |
| `icon` | the app **glyph/favicon** (a small square mark) | `icon_url` | 24px `object-contain` mark beside the name / rail tile |
| `art`  | the app **screenshot/preview** (a wide image)  | `art_url`  | `h-40 w-full object-cover` card image |

The API turns each relative path into a same-origin URL: `<app canonical url>/<path>`,
after `safe_relative_art_path` rejects anything that escapes the bundle (`..`, absolute,
off-origin, `\\`, `?`, `#`). The homepage helper is
`manifest_card_fields` + `CustomAppSummary::from_model_with_manifest` in
`crates/app/src/server/api/workspace_custom_apps.rs`. **Reuse that helper** — do not
re-derive URLs by hand.

## Non-negotiables

- **Manifest is the ONLY source.** Do **not** add a `<url>/favicon.ico` fetch, a
  headless-browser screenshot capture, an `icon`/`art`/`screenshot`/`favicon` DB column,
  or any per-surface image derivation. If a picture isn't in the manifest, the app has
  no picture — render the fallback, not a second lookup. (There is deliberately **no
  screenshot capture** yet; `art` is author-supplied only.)
- **The homepage's manifest approach is canonical; every other surface follows it.**
  Admin adopts what the homepage shows, never the reverse. The old admin-only
  `AppFavicon` (`<url>/favicon.ico`) mechanism is **removed** — admin reads `icon_url`
  from the API like the homepage does.
- **One shared render component per picture.** `AppMark` (icon → **monogram** fallback:
  the name's initial in a tile) and `AppArt` (art → **initial-letter tile** fallback).
  Both the launcher and the admin browser import the *same* components. A new surface
  showing an app icon imports `AppMark`; it does not hand-roll an `<img>` + fallback.
- **Fallbacks, not blanks.** No `icon_url` → monogram (never nothing). No `art_url` →
  letter tile. Both fallbacks derive from `app.name[0]` and use `bg-primary/10` +
  `text-primary`, so a picture-less app still reads as a first-class card.
- **List endpoints resolve manifests in BATCH.** Any endpoint returning many apps
  (admin `list_apps`/`list_my_apps`, homepage `list_custom_apps`) must resolve the
  per-app manifest with a single batched load (join `published_build_id` →
  `app_builds.manifest_json`, plus `apps.manifest_override`), never an N+1 per-app read.
  Manifest resolution is metadata: a failure yields empty icon/art, never a 500.

## Where it lives

- **Backend, homepage:** `workspace_custom_apps.rs` — `list_custom_apps`,
  `manifest_card_fields`, `safe_relative_art_path`, `from_model_with_manifest`.
- **Backend, admin:** `crates/app/src/server/api/admin/apps/handlers.rs` — `AppResponse`
  carries `icon_url`/`art_url`; `list_apps`/`list_my_apps`/the single-app read populate
  them via the shared helper + a batched manifest load.
- **Frontend shared:** `AppMark` + `AppArt` (icon/art with fallbacks) — one home,
  imported by both `pages/launcher/components/AppCard.tsx` and
  `pages/admin/AdminCustomApps/**`.
- **Manifest type:** `art` + `icon` on `OxyAppManifest`
  (`crates/app/src/server/api/custom_apps_manifest.rs`); mirror any change in the SDK
  manifest type + the custom-app author `oxy.d.ts`.

## Litmus test before adding image code

Ask: "Am I about to fetch/derive an app picture from anything other than
`icon_url`/`art_url`?" If yes (a favicon.ico probe, a screenshot job, a second column) —
stop; that's the divergence this skill exists to prevent. And: "Would a new surface
render the fallback the same as the homepage?" If not, route it through `AppMark`/`AppArt`.
