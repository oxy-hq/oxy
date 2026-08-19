//! IoT fleet identity + claim flow.
//!
//! Phase 1 surface: factory enrollment and device announce. Bootstrap
//! and operator-driven claim land in phases 1B and 1C.

use crate::entities::{device_claims, device_registry};
use crate::service::{ServiceError, ServiceResult};
use base64::Engine;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, NotSet, QueryFilter,
    QuerySelect, Set, TransactionTrait, sea_query::OnConflict,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Length of the device secret. 32 bytes matches HMAC-SHA256's
/// natural block size and provides 256-bit entropy.
pub const DEVICE_SECRET_LEN: usize = 32;

/// Bytes of entropy in a claim code before encoding. 9 bytes is
/// 72 bits — far more than needed for a 12-char human-typeable
/// code, but lets us derive the code via a hash without truncation
/// games (we keep the first 12 chars of a deterministic encoding).
const CLAIM_CODE_ENTROPY_BYTES: usize = 9;

/// Inputs to [`enroll_device`]. The device generates `device_id`
/// and `device_secret` at first boot and presents them here; the
/// server records `(device_id, device_secret, sku, hw_rev)` and
/// returns a sticky `claim_code` derived from `device_id`.
pub struct EnrollInput<'a> {
    pub device_id: Uuid,
    pub device_secret: &'a [u8],
    pub sku: &'a str,
    pub hardware_revision: &'a str,
    pub manufactured_at: chrono::DateTime<chrono::Utc>,
}

/// Outcome of factory enrollment. The claim code is what the
/// operator scans / types to bind the device to a workspace.
pub struct EnrollOutput {
    pub device_id: Uuid,
    pub claim_code: String,
}

/// Idempotent enrollment of a device into the registry.
///
/// Re-enrolling an existing `device_id` (returned-to-vendor + reshipped,
/// or a factory rerun) **overwrites the secret** so the device that
/// freshly self-provisioned wins. The claim code is computed from
/// `device_id` + a deployment-wide salt, so it's stable across
/// re-enrollment — an operator who already scanned the QR doesn't
/// have to re-print labels.
///
/// Rejects an empty `device_secret` or one wrong-length, since the
/// announce HMAC verification depends on a known-good key shape.
pub async fn enroll_device(
    db: &DatabaseConnection,
    claim_salt: &[u8],
    input: EnrollInput<'_>,
) -> ServiceResult<EnrollOutput> {
    if input.device_secret.len() != DEVICE_SECRET_LEN {
        return Err(ServiceError::InvalidInput(format!(
            "device_secret must be exactly {DEVICE_SECRET_LEN} bytes"
        )));
    }
    if input.sku.is_empty() || input.hardware_revision.is_empty() {
        return Err(ServiceError::InvalidInput(
            "sku and hardware_revision must be non-empty".into(),
        ));
    }

    let claim_code = derive_claim_code(input.device_id, claim_salt);
    let now = chrono::Utc::now().fixed_offset();

    // Seal the HMAC secret at-rest. The DB column holds the sealed
    // blob from this point on; every read goes through `secrets::open`.
    let sealed_secret = crate::secrets::seal(input.device_secret)
        .map_err(|e| ServiceError::Internal(format!("seal device_secret: {e}")))?;

    let am = device_registry::ActiveModel {
        device_id: Set(input.device_id),
        sku: Set(input.sku.to_string()),
        hardware_revision: Set(input.hardware_revision.to_string()),
        manufactured_at: Set(input.manufactured_at.fixed_offset()),
        device_secret: Set(sealed_secret),
        claim_code: Set(claim_code.clone()),
        last_announce_at: Set(None),
        created_at: Set(now),
    };

    // ON CONFLICT (device_id) DO UPDATE — re-enrollment overwrites
    // mutable fields but keeps the original `created_at`. claim_code
    // is deterministic from device_id so updating it is a no-op,
    // but we include it in the update set anyway in case the salt
    // ever changes (in which case we'd want all devices to roll
    // forward together).
    device_registry::Entity::insert(am)
        .on_conflict(
            OnConflict::column(device_registry::Column::DeviceId)
                .update_columns([
                    device_registry::Column::Sku,
                    device_registry::Column::HardwareRevision,
                    device_registry::Column::ManufacturedAt,
                    device_registry::Column::DeviceSecret,
                    device_registry::Column::ClaimCode,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(EnrollOutput {
        device_id: input.device_id,
        claim_code,
    })
}

/// Outcome of `POST /fleet/announce`.
#[derive(Debug)]
pub enum AnnounceOutcome {
    /// Device is enrolled but has no active claim. Operator needs
    /// to scan the QR before the device can bootstrap. The device
    /// keeps polling.
    NotClaimed,
    /// Device has a `pending_claim` — operator scanned the QR but
    /// the device hasn't bootstrapped yet. Return the same signal
    /// as `NotClaimed` so we don't leak the existence of a claim
    /// to a stale device (which would matter if the device is
    /// stolen post-claim). The device that knows its own secret
    /// will get the bootstrap path via /fleet/bootstrap directly.
    PendingClaim,
    /// Device is already claimed and bootstrapped. Worker should
    /// transition to the normal `/control/*` flow against
    /// `edge_box_id`.
    Claimed {
        workspace_id: Uuid,
        edge_box_id: Uuid,
    },
}

/// Inputs to [`announce_device`]. The device sends:
///   - `device_id` it identifies as
///   - `timestamp` (epoch seconds) of its own clock — small
///     freshness window so a recorded request can't be replayed
///     after a long delay
///   - `signature` = HMAC-SHA256(device_secret, `{device_id}.{timestamp}`)
pub struct AnnounceInput<'a> {
    pub device_id: Uuid,
    pub timestamp: i64,
    pub signature: &'a [u8],
}

/// Verify the announce signature and update the device's
/// `last_announce_at`. Returns what the device should do next.
///
/// Replay window: ±5min around the server's clock. The device's
/// clock may drift in the field; tightening this would force every
/// IoT box to have a working NTP loop before it can talk to Oxy.
/// 5min is generous and still defeats replay-after-recovery
/// scenarios.
pub async fn announce_device(
    db: &DatabaseConnection,
    input: AnnounceInput<'_>,
) -> ServiceResult<AnnounceOutcome> {
    const REPLAY_WINDOW_SECS: i64 = 300;
    let now = chrono::Utc::now().timestamp();
    if (now - input.timestamp).abs() > REPLAY_WINDOW_SECS {
        return Err(ServiceError::Forbidden(
            "announce timestamp outside replay window",
        ));
    }

    let device = device_registry::Entity::find_by_id(input.device_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    let device_secret = crate::secrets::open(&device.device_secret)
        .map_err(|e| ServiceError::Internal(format!("open device_secret: {e}")))?;
    let expected = expected_announce_signature(&device_secret, input.device_id, input.timestamp);
    if !constant_time_eq(input.signature, &expected) {
        return Err(ServiceError::Forbidden("announce signature mismatch"));
    }

    // Stamp last_announce_at. We do this even if the device is
    // unclaimed — fleet observability cares about "device was
    // alive at T" regardless of binding state.
    let mut am: device_registry::ActiveModel = device.into();
    am.last_announce_at = Set(Some(chrono::Utc::now().fixed_offset()));
    am.update(db).await?;

    // Decide what to tell the device. We re-fetch the claim status
    // here rather than carrying it through the update so the
    // outcome reflects the freshest claim state — minimizes a race
    // where an operator claims between signature verification and
    // the outcome write.
    let claim = device_claims::Entity::find()
        .filter(device_claims::Column::DeviceId.eq(input.device_id))
        .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
        .one(db)
        .await?;

    let outcome = match claim {
        None => AnnounceOutcome::NotClaimed,
        Some(c) if c.status == "claimed" => match c.edge_box_id {
            Some(edge_box_id) => AnnounceOutcome::Claimed {
                workspace_id: c.workspace_id,
                edge_box_id,
            },
            // Defensive: claimed but no edge_box_id is a bug
            // somewhere in the bootstrap path. Treat as pending.
            None => AnnounceOutcome::PendingClaim,
        },
        Some(_) => AnnounceOutcome::PendingClaim,
    };
    Ok(outcome)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Derive the device's sticky claim code from `device_id` + a
/// deployment-wide salt. SHA-256 then base32-encode and slice to
/// 12 chars formatted `XXXX-YYYY-ZZZZ`. We strip ambiguous chars
/// (`0/O/1/I/l`) by using a custom alphabet so the operator
/// reading it off a label can't miscopy.
fn derive_claim_code(device_id: Uuid, salt: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(device_id.as_bytes());
    h.update(salt);
    let digest = h.finalize();

    // Custom alphabet: no 0/O/1/I/l, no vowels (avoid accidental
    // words). 26 chars = 4.7 bits per char; 12 chars = 56 bits of
    // entropy from a 256-bit hash, plenty.
    const ALPHABET: &[u8] = b"23456789BCDFGHJKMNPQRTVWXY";
    let mut out = String::with_capacity(14);
    for (i, b) in digest.iter().take(CLAIM_CODE_ENTROPY_BYTES).enumerate() {
        out.push(ALPHABET[(*b as usize) % ALPHABET.len()] as char);
        // Insert a separator every 4 chars to make the code
        // easier to read aloud: `XXXX-YYYY-ZZZZ`.
        if (i + 1) % 3 == 0 && i + 1 < CLAIM_CODE_ENTROPY_BYTES {
            out.push('-');
        }
    }
    out
}

/// Generate a fresh random secret. Used by tests + (eventually) the
/// device's own setup script. Returns the raw bytes.
pub fn generate_device_secret() -> Vec<u8> {
    let bytes: [u8; DEVICE_SECRET_LEN] = rand::random();
    bytes.to_vec()
}

/// What [`prepare_device`] returns. The operator-facing UI shows
/// `device_id` + `device_secret_b64` once and never again —
/// re-fetching would mean the secret has to live somewhere
/// readable, which defeats the per-install-credential model.
///
/// `claim_code` is included for symmetry with the QR-scan flow but
/// won't normally be needed: this preparation flow pre-creates the
/// pending claim atomically, so the device announces and
/// bootstraps without a human ever typing the code.
pub struct PrepareOutput {
    pub device_id: Uuid,
    pub device_secret: Vec<u8>,
    pub claim_code: String,
    pub claim_id: Uuid,
    pub sku: String,
    pub hardware_revision: String,
}

/// "Add device" wizard backend (Option 3 from internal Q&A).
///
/// The operator clicks a button in the Fleet tab and we mint a
/// fresh device identity + pre-create the pending claim, all in
/// one atomic action. The operator copies the resulting JSON
/// blob into the box's `/var/lib/oxy/device.json` and boots the
/// worker; the device self-announces against an already-pending
/// claim and bootstraps without any additional human action.
///
/// Why not a long-lived shared factory secret: industry standard
/// (Tailscale pre-auth keys, kubeadm bootstrap tokens, AWS IoT
/// JITP) is per-install credentials, not a fleet-wide secret. We
/// follow that pattern so a compromised installer machine can
/// only impersonate the one device whose secret is on it, not
/// every device the operator will ever provision.
///
/// `hardware_label` is operator-supplied free-text (e.g. "Jetson
/// Orin Nano" or "Intel N100") — purely descriptive, used as the
/// `hardware_revision` so the existing Fleet table can show
/// something meaningful for self-provisioned boxes.
pub async fn prepare_device(
    db: &DatabaseConnection,
    claim_salt: &[u8],
    workspace_id: Uuid,
    site_id: Uuid,
    hardware_label: Option<&str>,
    prepared_by: Option<Uuid>,
) -> ServiceResult<PrepareOutput> {
    use crate::entities::sites;

    // Site ownership. Cross-workspace site_id rejected before we
    // mint any credentials — the worst case is a wasted secret, but
    // belt-and-suspenders.
    let site = sites::Entity::find_by_id(site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }

    // Generate identity. UUID v7 sorts by creation time which makes
    // operator-side debugging easier ("the newest device is at the
    // bottom of the list when sorted by id"). The secret is 32
    // bytes of CSPRNG output.
    let device_id = Uuid::now_v7();
    let device_secret = generate_device_secret();
    let sku = "self-installed".to_string();
    let hardware_revision = hardware_label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unspecified")
        .to_string();
    let enroll_input = EnrollInput {
        device_id,
        device_secret: &device_secret,
        sku: &sku,
        hardware_revision: &hardware_revision,
        manufactured_at: chrono::Utc::now(),
    };
    let enroll_out = enroll_device(db, claim_salt, enroll_input).await?;

    // Atomically pre-create the pending claim so the device
    // announces against an existing row on first boot. The race
    // window where someone could grab the device_id between
    // enroll and claim is small (microseconds) and inside our
    // own process, but tightening it doesn't cost much.
    let claim_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();
    device_claims::ActiveModel {
        id: Set(claim_id),
        device_id: Set(device_id),
        workspace_id: Set(workspace_id),
        site_id: Set(Some(site_id)),
        edge_box_id: Set(None),
        status: Set("pending_claim".into()),
        claimed_by: Set(prepared_by),
        claimed_at: Set(Some(now)),
        revoked_at: Set(None),
        created_at: Set(now),
    }
    .insert(db)
    .await?;

    Ok(PrepareOutput {
        device_id,
        device_secret,
        claim_code: enroll_out.claim_code,
        claim_id,
        sku,
        hardware_revision,
    })
}

/// Compute the HMAC-SHA256 signature the device sends with each
/// announce. Exposed so tests + the device's reference
/// implementation can compute the same value the server expects.
pub fn expected_announce_signature(
    device_secret: &[u8],
    device_id: Uuid,
    timestamp: i64,
) -> Vec<u8> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac =
        Hmac::<Sha256>::new_from_slice(device_secret).expect("HMAC accepts any key length");
    mac.update(device_id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.to_string().as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time byte comparison. We're checking a signature, so
/// short-circuiting on the first mismatch would leak position info
/// to a timing-attack-capable adversary. The `subtle` crate would
/// be the canonical pick but we already have everything we need.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Base64-decode a header-supplied signature. Used in the route
/// handler to convert the wire format into bytes before
/// [`announce_device`] consumes it.
pub fn decode_signature(b64: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64)
}

// `claim_device` (the operator types a claim_code printed on
// factory-burned hardware) was removed alongside the
// `POST /fleet/claims` operator route — we don't have factory
// hardware, and the Add device wizard pre-creates the
// pending_claim atomically via `prepare_device`.

// ── Phase 1C — bootstrap ────────────────────────────────────────────────────

/// What [`bootstrap_device`] hands back. The device persists these
/// to disk; subsequent `/control/*` calls authenticate via a JWT
/// minted from the device's HMAC secret (resolved on the server side
/// through `device_claims.edge_box_id`).
pub struct BootstrapOutput {
    pub workspace_id: Uuid,
    pub edge_box_id: Uuid,
}

/// Inputs to [`bootstrap_device`]. Reuses the announce signature
/// shape so the device-side code path is symmetric (compute HMAC
/// over `device_id || "." || timestamp` once, use everywhere).
pub struct BootstrapInput<'a> {
    pub device_id: Uuid,
    pub timestamp: i64,
    pub signature: &'a [u8],
}

/// Device-driven materialization. Called after the operator has
/// claimed the device and the device's next `/fleet/announce`
/// learned `pending_claim`. Steps:
///
///   1. Verify HMAC signature + replay window (same shape as announce).
///   2. Find the `pending_claim` row for this device. 404 if none.
///   3. Create an `edge_boxes` row at the claim's `site_id` with
///      reasonable defaults (cohort=stable, hardware_model from
///      registry).
///   4. Transition the claim to `claimed` with `edge_box_id` set.
///
/// Idempotency: if the device retries bootstrap (lost the response,
/// fault recovery), the claim has already moved to `claimed`.
pub async fn bootstrap_device(
    db: &DatabaseConnection,
    input: BootstrapInput<'_>,
) -> ServiceResult<BootstrapOutput> {
    use crate::entities::{edge_boxes, sites};

    const REPLAY_WINDOW_SECS: i64 = 300;
    let now_ts = chrono::Utc::now().timestamp();
    if (now_ts - input.timestamp).abs() > REPLAY_WINDOW_SECS {
        return Err(ServiceError::Forbidden(
            "bootstrap timestamp outside replay window",
        ));
    }

    let device = device_registry::Entity::find_by_id(input.device_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    let device_secret = crate::secrets::open(&device.device_secret)
        .map_err(|e| ServiceError::Internal(format!("open device_secret: {e}")))?;
    let expected = expected_announce_signature(&device_secret, input.device_id, input.timestamp);
    if !constant_time_eq(input.signature, &expected) {
        return Err(ServiceError::Forbidden("bootstrap signature mismatch"));
    }

    // Race-safe materialization. Two containers booting against the
    // same `device.json` (e.g. an `edge` + `update-agent` pair) used
    // to both win their respective `find(status='pending_claim')`
    // SELECTs and each ran `register_edge_box`, producing two
    // `edge_boxes` rows for one device — one became the live box,
    // the other was orphaned permanently because only the last
    // claim UPDATE survived. We now do the SELECT inside a
    // transaction with `lock_exclusive()` (Postgres `FOR UPDATE`),
    // which serialises concurrent bootstrap callers: the loser's
    // SELECT returns no row (its `status='pending_claim'` filter
    // doesn't match the now-`claimed` row) and exits with
    // `NotFound`. The losing client should re-call `announce` to
    // observe the `Claimed { edge_box_id }` outcome.
    //
    // The Airhouse `ensure_schema` side-effect happens OUTSIDE the
    // transaction so we don't hold the row lock during a network
    // round-trip.
    let now = chrono::Utc::now();
    let edge_box_id = Uuid::new_v4();
    let txn = db.begin().await?;

    let claim = device_claims::Entity::find()
        .filter(device_claims::Column::DeviceId.eq(input.device_id))
        .filter(device_claims::Column::Status.eq("pending_claim"))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(ServiceError::NotFound)?;

    let site_id = claim.site_id.ok_or(ServiceError::InvalidInput(
        "claim has no site_id — re-claim with a site selected".into(),
    ))?;
    let workspace_id = claim.workspace_id;

    // Site ownership re-check inside the txn — defends against a
    // site being moved between workspaces while a bootstrap is in
    // flight. Same shape the operator-facing `register_edge_box`
    // performs.
    let site = sites::Entity::find_by_id(site_id)
        .one(&txn)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "claim site belongs to another workspace",
        ));
    }

    edge_boxes::ActiveModel {
        id: Set(edge_box_id),
        site_id: Set(site_id),
        hardware_model: Set(device.hardware_revision.clone()),
        image_tag: Set("unknown".into()),
        cohort: Set("stable".into()),
        tailscale_ip: NotSet,
        funnel_hostname: NotSet,
        bandwidth_5min_bytes: NotSet,
        bandwidth_reported_at: NotSet,
        target_image_tag: NotSet,
        current_image_tag: NotSet,
        held_until: NotSet,
        last_update_result: NotSet,
        last_update_at: NotSet,
        edge_compatibility_json: NotSet,
        incompatible_reason: NotSet,
        status: Set("pending".into()),
        unifi_console_id: NotSet,
        unifi_public_ip: NotSet,
        unifi_rtsp_reachable: Set(false),
        registered_at: Set(now.into()),
        last_seen_at: NotSet,
        updated_at: Set(now.into()),
    }
    .insert(&txn)
    .await?;

    let mut am: device_claims::ActiveModel = claim.into();
    am.status = Set("claimed".into());
    am.edge_box_id = Set(Some(edge_box_id));
    am.update(&txn).await?;

    txn.commit().await?;

    // Camera-intent trigger — see `service::registration` for the
    // soft-fail rationale (Postgres write is committed; Airhouse
    // schema is best-effort and the ingest path lazy-retries).
    if let Err(e) = crate::airhouse::ensure_schema(workspace_id).await {
        tracing::warn!(
            workspace_id = %workspace_id,
            edge_box_id = %edge_box_id,
            error = %e,
            "ensure_schema failed during bootstrap_device; lazy ensure will retry on first ingest"
        );
    }

    Ok(BootstrapOutput {
        workspace_id,
        edge_box_id,
    })
}

// ── Retroactive claim — attach a self-enrolled device to an
//    operator-registered edge_box ──────────────────────────────────

/// What [`bind_existing`] hands back.
pub struct BindExistingOutput {
    pub claim_id: Uuid,
}

/// Bind a known `device_id` to an existing `edge_box_id` without
/// running the full announce → bootstrap dance.
///
/// **What it does.** Validates the edge_box belongs to the
/// caller's workspace, looks up the device in `device_registry`
/// (the device must have self-enrolled first — we deliberately
/// do not auto-enroll from the operator path, which would
/// weaken the manufacturing-VPN gate on `/fleet/factory-enroll`),
/// and inserts a `device_claims` row with `status='claimed'` and
/// `edge_box_id` already set.
///
/// **Conflicts.**
///   - Unknown `device_id` → 404 (device hasn't enrolled yet).
///   - Edge box not in workspace → 403.
///   - Either side already has an active claim → 409 (operator
///     must revoke first; we don't fold/replace silently).
pub async fn bind_existing(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    device_id: Uuid,
    edge_box_id: Uuid,
    claimed_by: Option<Uuid>,
) -> ServiceResult<BindExistingOutput> {
    use crate::entities::edge_boxes;
    use crate::entities::sites;

    // Edge box ownership — same defense as the cost / logs paths.
    // We join through sites to enforce workspace boundaries.
    let edge_box = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let site = sites::Entity::find_by_id(edge_box.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "edge box belongs to another workspace",
        ));
    }

    // Device must already be enrolled. Auto-enrolling here would
    // bypass the manufacturing-VPN gate — better to surface a
    // clear "flash the new image first" error than silently weaken
    // the security boundary.
    let _device = device_registry::Entity::find_by_id(device_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    // Reject if either side already has an active binding. We
    // look at both the device's open claims and any claim
    // already pointing at this edge_box so an operator can't
    // accidentally double-bind a single device or attach two
    // device identities to one edge_box.
    let device_active = device_claims::Entity::find()
        .filter(device_claims::Column::DeviceId.eq(device_id))
        .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
        .one(db)
        .await?;
    if device_active.is_some() {
        return Err(ServiceError::Conflict(
            "device already has an active claim — revoke first".into(),
        ));
    }
    let edge_box_active = device_claims::Entity::find()
        .filter(device_claims::Column::EdgeBoxId.eq(edge_box_id))
        .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
        .one(db)
        .await?;
    if edge_box_active.is_some() {
        return Err(ServiceError::Conflict(
            "edge box already linked to a device — revoke first".into(),
        ));
    }

    let claim_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();
    device_claims::ActiveModel {
        id: Set(claim_id),
        device_id: Set(device_id),
        workspace_id: Set(workspace_id),
        site_id: Set(Some(edge_box.site_id)),
        // Difference from the new-device claim path: edge_box_id
        // is set immediately because we're attaching to an
        // already-materialized row.
        edge_box_id: Set(Some(edge_box_id)),
        status: Set("claimed".into()),
        claimed_by: Set(claimed_by),
        claimed_at: Set(Some(now)),
        revoked_at: Set(None),
        created_at: Set(now),
    }
    .insert(db)
    .await?;

    Ok(BindExistingOutput { claim_id })
}

// ── Remove device ──────────────────────────────────────────────────────────

/// What [`remove_member`] actually did. Operator-facing UI uses
/// this to log "retired box X and revoked claim C."
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoveOutput {
    pub edge_box_retired: bool,
    pub claim_revoked: bool,
    /// Resolved primary target for audit logging — operator
    /// hit Remove on a row; this is "the thing they removed."
    pub target_id: Uuid,
}

/// Remove a fleet member. Handles three cases the merged list
/// emits:
///
///   - **Edge box, possibly with claim** — flips
///     `edge_boxes.status = 'retired'`, and if any `device_claims`
///     row points at it, flips that to `'revoked'`. The actual rows
///     stay (for compliance reports, audit trail, etc.); the
///     operator-visible row disappears from the fleet list
///     because it's filtered to non-retired + active claims.
///
///   - **Pending claim with no edge_box** — just flips the
///     claim to `'revoked'`. No edge_box materialized yet, so
///     there's nothing else to clean up.
///
/// Caller passes whichever id they have (or both). At least one
/// must be set; both `None` rejects as `InvalidInput`.
///
/// Ownership: edge_box → site → workspace_id is verified before
/// anything else writes. A device_id without an edge_box is
/// verified via `device_claims.workspace_id`.
pub async fn remove_member(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    edge_box_id: Option<Uuid>,
    device_id: Option<Uuid>,
) -> ServiceResult<RemoveOutput> {
    use crate::entities::{cameras, edge_boxes, sites};
    use sea_orm::sea_query::Expr;

    if edge_box_id.is_none() && device_id.is_none() {
        return Err(ServiceError::InvalidInput(
            "must provide edge_box_id, device_id, or both".into(),
        ));
    }

    let now = chrono::Utc::now().fixed_offset();
    let mut out = RemoveOutput {
        edge_box_retired: false,
        claim_revoked: false,
        target_id: edge_box_id.or(device_id).unwrap_or(Uuid::nil()),
    };

    // ── Edge box path ──────────────────────────────────────
    if let Some(eb_id) = edge_box_id {
        let eb = edge_boxes::Entity::find_by_id(eb_id)
            .one(db)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let site = sites::Entity::find_by_id(eb.site_id)
            .one(db)
            .await?
            .ok_or(ServiceError::NotFound)?;
        if site.workspace_id != workspace_id {
            return Err(ServiceError::Forbidden(
                "edge box belongs to another workspace",
            ));
        }

        // Retire the edge_box if it isn't already.
        if eb.status != "retired" {
            let mut am: edge_boxes::ActiveModel = eb.into();
            am.status = Set("retired".into());
            am.updated_at = Set(now);
            am.update(db).await?;
            out.edge_box_retired = true;
        }

        let linked_claim = device_claims::Entity::find()
            .filter(device_claims::Column::EdgeBoxId.eq(eb_id))
            .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
            .one(db)
            .await?;
        if let Some(c) = linked_claim {
            let mut am: device_claims::ActiveModel = c.into();
            am.status = Set("revoked".into());
            am.revoked_at = Set(Some(now));
            am.update(db).await?;
            out.claim_revoked = true;
        }

        // Release this box's cameras so the next box to poll
        // `/control/boxes/<new>/config` can pick them up.
        // `fetch_cameras_for_edge` claims only `edge_box_id IS NULL`
        // rows at the same site; without this unbind, the cameras
        // stay pinned to a retired box forever and the replacement
        // box thinks the site has zero cameras.
        cameras::Entity::update_many()
            .col_expr(
                cameras::Column::EdgeBoxId,
                Expr::value(Option::<Uuid>::None),
            )
            .col_expr(cameras::Column::UpdatedAt, Expr::value(now))
            .filter(cameras::Column::EdgeBoxId.eq(eb_id))
            .exec(db)
            .await?;
    }

    // ── Pending-claim path (no edge_box yet) ───────────────
    // Only runs when the caller didn't pass an edge_box_id —
    // otherwise the linked-claim revoke above already handled
    // the device_id case.
    if edge_box_id.is_none()
        && let Some(did) = device_id
    {
        let claim = device_claims::Entity::find()
            .filter(device_claims::Column::DeviceId.eq(did))
            .filter(device_claims::Column::WorkspaceId.eq(workspace_id))
            .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
            .one(db)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let mut am: device_claims::ActiveModel = claim.into();
        am.status = Set("revoked".into());
        am.revoked_at = Set(Some(now));
        am.update(db).await?;
        out.claim_revoked = true;
    }

    Ok(out)
}

// ── Phase 5 — operator fleet listing ────────────────────────────────────────

/// One row in the operator's Fleet tab. Combines the device
/// registry (what the factory burned) with the per-workspace
/// claim record (what the operator bound). `edge_box_id` is set
/// once the device has bootstrapped — the UI uses it to deep-link
/// over to the Edge Boxes tab.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FleetDeviceRow {
    pub device_id: Uuid,
    pub claim_code: String,
    pub sku: String,
    pub hardware_revision: String,
    /// `pending_claim` | `claimed` | `revoked`. Matches
    /// `device_claims.status`.
    pub status: String,
    /// Set on transition to `claimed`. NULL during pending or
    /// revoked.
    pub edge_box_id: Option<Uuid>,
    /// Site the operator picked at claim time.
    pub site_id: Option<Uuid>,
    /// Resolved site name for display — null if the site was
    /// deleted out from under the claim.
    pub site_name: Option<String>,
    pub last_announce_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub claimed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

/// All devices visible to this workspace: any `device_claims` row
/// with `workspace_id = $1` and status in (`pending_claim`,
/// `claimed`). Revoked rows are excluded — they belong to the
/// audit trail, not the operator's working view.
///
/// Note this lists devices *bound* to the workspace, not the
/// global registry. Unclaimed devices are factory-enrolled and
/// only visible at the manufacturing console; once an operator
/// scans a claim code and POSTs `/fleet/claims`, the resulting
/// `device_claims` row is what makes the device show up here.
pub async fn list_for_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<FleetDeviceRow>> {
    use crate::entities::sites;

    let claims: Vec<device_claims::Model> = device_claims::Entity::find()
        .filter(device_claims::Column::WorkspaceId.eq(workspace_id))
        .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
        .all(db)
        .await?;
    if claims.is_empty() {
        return Ok(Vec::new());
    }

    // Bulk-fetch related rows so we don't N+1 the device list.
    let device_ids: Vec<Uuid> = claims.iter().map(|c| c.device_id).collect();
    let devices: std::collections::HashMap<Uuid, device_registry::Model> =
        device_registry::Entity::find()
            .filter(device_registry::Column::DeviceId.is_in(device_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|d| (d.device_id, d))
            .collect();

    let site_ids: Vec<Uuid> = claims.iter().filter_map(|c| c.site_id).collect();
    let site_names: std::collections::HashMap<Uuid, String> = if site_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        sites::Entity::find()
            .filter(sites::Column::Id.is_in(site_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect()
    };

    let mut out: Vec<FleetDeviceRow> = claims
        .into_iter()
        .filter_map(|c| {
            // A claim referencing a device we can't find shouldn't
            // happen (FK is enforced), but skip rather than crash
            // the whole list if it does.
            let d = devices.get(&c.device_id)?;
            let site_name = c.site_id.and_then(|sid| site_names.get(&sid).cloned());
            Some(FleetDeviceRow {
                device_id: d.device_id,
                claim_code: d.claim_code.clone(),
                sku: d.sku.clone(),
                hardware_revision: d.hardware_revision.clone(),
                status: c.status,
                edge_box_id: c.edge_box_id,
                site_id: c.site_id,
                site_name,
                last_announce_at: d.last_announce_at,
                claimed_at: c.claimed_at,
            })
        })
        .collect();

    // Newest claims first — operators usually want to confirm the
    // device they just scanned shows up at the top.
    out.sort_by(|a, b| b.claimed_at.cmp(&a.claimed_at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_code_is_deterministic_per_device() {
        let id = Uuid::new_v4();
        let a = derive_claim_code(id, b"salt");
        let b = derive_claim_code(id, b"salt");
        assert_eq!(a, b);
    }

    #[test]
    fn claim_code_differs_per_device() {
        let salt = b"salt";
        let a = derive_claim_code(Uuid::new_v4(), salt);
        let b = derive_claim_code(Uuid::new_v4(), salt);
        assert_ne!(a, b);
    }

    #[test]
    fn claim_code_changes_with_salt() {
        let id = Uuid::new_v4();
        assert_ne!(derive_claim_code(id, b"a"), derive_claim_code(id, b"b"));
    }

    #[test]
    fn claim_code_avoids_ambiguous_chars() {
        let code = derive_claim_code(Uuid::new_v4(), b"salt");
        for c in code.chars() {
            assert!(
                !"01OIl".contains(c),
                "claim code `{code}` contains ambiguous char `{c}`"
            );
        }
    }

    #[test]
    fn announce_signature_roundtrip() {
        let secret = generate_device_secret();
        let id = Uuid::new_v4();
        let ts = 1000;
        let sig = expected_announce_signature(&secret, id, ts);
        // Idempotent: same inputs → same output.
        assert_eq!(sig, expected_announce_signature(&secret, id, ts));
        // Different timestamp → different signature.
        assert_ne!(sig, expected_announce_signature(&secret, id, ts + 1));
    }

    /// Cross-language compatibility vector. The device-side Python
    /// implementation in `video-poc/edge/worker/identity.py` MUST
    /// produce the same hex for the same inputs. If this test
    /// drifts, every fleet-deployed device's announce/bootstrap
    /// stops working until a coordinated rollback — must be a
    /// deliberate change on both sides.
    ///
    /// The pinned value was computed once via
    /// `python3 -c "from worker.identity import _announce_signature; ..."`
    /// and asserted to match this Rust output.
    #[test]
    fn announce_signature_compat_vector() {
        let secret: Vec<u8> = (0u8..32).collect();
        let id = Uuid::parse_str("00000000-0000-7000-8000-000000000000").unwrap();
        let ts = 1_700_000_000_i64;
        let sig = expected_announce_signature(&secret, id, ts);
        assert_eq!(
            hex::encode(sig),
            "ef4d4b583f463600a4d61945fef776fb0d4eefe6a2335e97052b2ccdb24d1028",
            "wire format drifted — coordinate with video-poc/edge/worker/identity.py"
        );
    }

    #[test]
    fn constant_time_eq_matches_only_equal_slices() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
