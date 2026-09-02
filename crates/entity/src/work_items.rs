//! The assignment graph.
//!
//! One table, five product surfaces. Tasks, Site visits, Location launcher,
//! Training and Compliance all reduce to *somebody owes somebody a piece of
//! work at a place by a time* — and building that once is the whole argument
//! for it being a platform entity rather than five app tables that each
//! re-derive "assigned to me" and "supervised by me".

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "work_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    #[sea_orm(indexed)]
    pub location_id: Option<Uuid>,
    pub title: String,
    pub body: Option<String>,

    /// Assigned to a person…
    pub assignee_user_id: Option<Uuid>,
    /// …or to whoever holds a role here.
    ///
    /// Both are needed and they are not the same fact. "The closing checklist"
    /// belongs to whoever is on shift, which is a role; "re-test the sanitiser
    /// you logged wrong" belongs to a person.
    ///
    /// **Neither column is guaranteed non-null, and no schema check enforces
    /// that one of them is.** An earlier draft had one; it was removed because
    /// both columns are `ON DELETE SET NULL`, so deleting the user or role an
    /// item was solely assigned to would null the last non-null column and make
    /// the DELETE fail — rejecting the deletion, not the assignment.
    ///
    /// The invariant is enforced at CREATION, in the handler, with a 400 that
    /// names the missing field. A lapsed assignment is a reachable and correct
    /// state — when somebody leaves, their open work becomes nobody's job, which
    /// is exactly what a manager needs surfaced. The `work_items_unassigned`
    /// partial index exists to find it. Do not write code that assumes
    /// `assignee_user_id.is_some() || assignee_role_id.is_some()` holds for
    /// every row.
    pub assignee_role_id: Option<Uuid>,
    pub supervisor_id: Option<Uuid>,

    pub due_at: Option<DateTimeWithTimeZone>,
    /// `open` | `in_progress` | `done` | `cancelled`.
    pub status: String,
    pub priority: i16,

    /// Why this exists — polymorphic on purpose.
    ///
    /// Work arrives from a failed form answer, a site-visit finding, a launcher
    /// template, a training path, a document expiry: five kinds today and more
    /// later. Five nullable foreign keys would add a column per source forever.
    /// A `(kind, id)` pair costs referential integrity, which is the right
    /// trade for a field that exists to answer "why does this task exist"
    /// rather than to be joined on.
    pub source_kind: Option<String>,
    pub source_id: Option<String>,

    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub completed_at: Option<DateTimeWithTimeZone>,
    pub completed_by: Option<Uuid>,

    #[sea_orm(
        belongs_to,
        from = "location_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub locations: BelongsTo<Option<super::locations::Entity>>,
}

impl Model {
    pub fn is_open(&self) -> bool {
        matches!(self.status.as_str(), "open" | "in_progress")
    }

    /// Overdue relative to a caller-supplied "now".
    ///
    /// The clock is a parameter rather than read here so this stays pure and
    /// testable — an overdue calculation that reads the wall clock can only be
    /// tested by waiting.
    pub fn is_overdue(&self, now: DateTimeWithTimeZone) -> bool {
        self.is_open() && self.due_at.is_some_and(|d| d < now)
    }
}

impl ActiveModelBehavior for ActiveModel {}
