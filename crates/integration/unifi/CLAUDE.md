# oxy-unifi

Outbound HTTP client for the UniFi (Ubiquiti) Site Manager API at
`api.ui.com` plus the Protect proxy under it.

This crate is **stateless** — it does not own SeaORM entities, a
migrator, or persistent storage. It's a thin SDK that callers
(notably `oxy-cameras`'s onboarding service) use to enumerate sites
and cameras, and to fetch streamable RTSPS URLs once an owner key is
provided.

## Dependency rules

- **No upward deps.** Must not import `oxy`, `oxy-shared`,
  `oxy-cameras`, or any platform crate. Pure SDK.
- **External crates only.** Allowed: `reqwest`, `serde`, `tokio`,
  `thiserror`, `tracing`, `url`. Adding anything else needs a reason.

## What's in here

- `lib.rs` — `UnifiClient` (X-API-KEY auth, base URL `api.ui.com`)
- `hosts.rs` — `GET /v1/hosts`, `GET /v1/hosts/{id}` (list controllers, fetch detail)
- `devices.rs` — `GET /v1/devices` (full camera/network inventory across the account)
- `protect.rs` — `/v1/connector/consoles/{console_id}/proxy/protect/integration/v1/*`
  (requires owner permission; returns 403 with admin-only keys)
- `errors.rs` — `UnifiError` enum (`Forbidden`, `NotFound`, `RateLimited`, etc.)

## What's NOT in here

- Anything that touches our own database. The onboarding *service*
  lives in `crates/cameras/src/service/onboarding.rs` and uses this
  client.
- Mappings from UniFi shapes to our `sites` / `cameras` schema. That
  translation belongs in the cameras crate.

## Reference

- [`internal-docs/video-processing-fleet-architecture.md`](../../../internal-docs/video-processing-fleet-architecture.md)
- UniFi developer docs (SPA, hard to fetch programmatically):
  <https://developer.ui.com/protect/v7.1.46/>
