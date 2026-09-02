//! How a human proves who they are — one row per credential, several per user.
//!
//! Email used to *be* the identity: `users.email` was the unique key every
//! provider collapsed onto. That works until the people using an operational app
//! are hourly staff with no mailbox. This table demotes email from *the* key to
//! *a* credential, so a worker who later gets a work address **adds a row** here
//! rather than becoming a second person.
//!
//! `org_id` is the scope, and it is what makes a 4-digit PIN viable: a PIN is
//! unique inside one org, never globally. Email credentials carry a NULL
//! `org_id` and stay globally unique — enforced by two *partial* unique indexes,
//! because a single `UNIQUE (kind, org_id, identifier)` would accept two rows of
//! `('email', NULL, 'a@b.com')` (Postgres treats NULLs as distinct there).
//!
//! Design record: `internal-docs/frontline-identity.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "user_credentials")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// `email` | `phone` | `pin`. A DB check constraint pins the set — this is
    /// a `String` rather than an enum so adding a kind is a migration, not a
    /// coordinated deploy across every reader.
    pub kind: String,
    /// The org a scoped credential belongs to. NULL for globally-unique kinds
    /// (`email`), NOT NULL for `pin` — both enforced in the schema.
    pub org_id: Option<Uuid>,
    /// The address, the E.164 number, or the login name shown on a kiosk.
    pub identifier: String,
    /// argon2 for `pin`. NULL where possession is the proof (a mailed link, an
    /// SMS code) and there is no secret at rest.
    pub secret_hash: Option<String>,
    /// Consecutive failures within the current window. Reset by a successful
    /// verify, and by a lockout **lapsing** — but never merely by time passing
    /// inside a window, because a slow guesser is still a guesser.
    ///
    /// The lapse reset is what stops a worker who locked out once from being a
    /// single mistyped digit away from another full window, for ever.
    pub failed_attempts: i32,
    /// Set once [`Self::failed_attempts`] crosses the policy's ceiling. A
    /// verify against a locked credential fails without checking the secret,
    /// so the lockout costs the attacker the whole window rather than one try.
    pub locked_until: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
