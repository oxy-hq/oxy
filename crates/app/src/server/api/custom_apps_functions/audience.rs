//! Who can reach a custom app: the one audience rule `ctx.org.people()`,
//! `ctx.org.assignments()` and `ctx.notify` share, so the three cannot drift.
//!
//! The rule is [`Standing::admits`]. It is read two ways, and both ask it:
//! [`app_audience`] — every person one app can reach, from the app's org — and
//! [`PersonReach`] — one person's standing toward many apps, from the person's
//! own rows (the notification catalogue's question, asked for every app at once
//! in three queries rather than one roster read per app).
//!
//! Outside the `custom-app-functions` feature on purpose: the notification
//! service resolves recipients with it, and compiles in every configuration.

use std::collections::HashSet;

use entity::{app_members, org_frontline_members, org_members};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect};
use uuid::Uuid;

/// What the audience rule reads about one person and one app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Standing {
    /// A member of the app's org.
    pub member: bool,
    /// An active frontline worker of the app's org.
    pub active_crew: bool,
    /// Holds an `app_members` grant on the app.
    pub granted: bool,
}

impl Standing {
    /// THE rule: the org's members, plus its active frontline workers holding
    /// a grant on the app. Nothing else — not the app's visibility, not a team
    /// grant — places a person in an app's audience.
    pub fn admits(self) -> bool {
        self.member || (self.active_crew && self.granted)
    }
}

/// Who can reach this app, and the membership rows that said so.
pub struct Audience {
    pub ids: HashSet<Uuid>,
    /// The org's members — every one is in `ids`. Returned so a caller that
    /// needs their roles does not read the table a second time.
    pub members: Vec<org_members::Model>,
}

/// The people who can reach this app: every person [`Standing::admits`] for it
/// — the one audience rule `people()` and `assignments()` share, factored so it
/// cannot drift between them.
pub async fn app_audience(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<Audience, String> {
    let members = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .all(db)
        .await
        .map_err(|e| format!("ctx.org: {e}"))?;
    let member_ids: HashSet<Uuid> = members.iter().map(|m| m.user_id).collect();
    let workers: HashSet<Uuid> = org_frontline_members::Entity::find()
        .filter(org_frontline_members::Column::OrgId.eq(org_id))
        .filter(org_frontline_members::Column::Status.eq(org_frontline_members::STATUS_ACTIVE))
        .all(db)
        .await
        .map_err(|e| format!("ctx.org: {e}"))?
        .into_iter()
        .map(|w| w.user_id)
        .collect();
    // A grant decides nothing for someone who is not active crew, so the
    // grants are read only when there is some.
    let granted: HashSet<Uuid> = if workers.is_empty() {
        HashSet::new()
    } else {
        app_members::Entity::find()
            .filter(app_members::Column::AppId.eq(app_id))
            .all(db)
            .await
            .map_err(|e| format!("ctx.org: {e}"))?
            .into_iter()
            .map(|g| g.user_id)
            .collect()
    };
    let ids = member_ids
        .iter()
        .chain(&workers)
        .copied()
        .filter(|u| {
            Standing {
                member: member_ids.contains(u),
                active_crew: workers.contains(u),
                granted: granted.contains(u),
            }
            .admits()
        })
        .collect();
    Ok(Audience { ids, members })
}

/// One person's rows the audience rule reads, for every app at once: the orgs
/// they are a member of, the orgs they are active crew in, and the apps they
/// hold a grant on.
#[derive(Debug, Clone, Default)]
pub struct PersonReach {
    pub member_orgs: HashSet<Uuid>,
    pub crew_orgs: HashSet<Uuid>,
    pub granted_apps: HashSet<Uuid>,
}

impl PersonReach {
    /// `user_id`'s rows, in at most three queries whatever the number of apps
    /// or the size of any org. Grants are read only for active crew, as in
    /// [`app_audience`].
    pub async fn load(db: &DatabaseConnection, user_id: Uuid) -> Result<Self, DbErr> {
        let member_orgs: Vec<Uuid> = org_members::Entity::find()
            .select_only()
            .column(org_members::Column::OrgId)
            .filter(org_members::Column::UserId.eq(user_id))
            .into_tuple()
            .all(db)
            .await?;
        let crew_orgs: Vec<Uuid> = org_frontline_members::Entity::find()
            .select_only()
            .column(org_frontline_members::Column::OrgId)
            .filter(org_frontline_members::Column::UserId.eq(user_id))
            .filter(org_frontline_members::Column::Status.eq(org_frontline_members::STATUS_ACTIVE))
            .into_tuple()
            .all(db)
            .await?;
        let granted_apps: Vec<Uuid> = if crew_orgs.is_empty() {
            Vec::new()
        } else {
            app_members::Entity::find()
                .select_only()
                .column(app_members::Column::AppId)
                .filter(app_members::Column::UserId.eq(user_id))
                .into_tuple()
                .all(db)
                .await?
        };
        Ok(Self {
            member_orgs: member_orgs.into_iter().collect(),
            crew_orgs: crew_orgs.into_iter().collect(),
            granted_apps: granted_apps.into_iter().collect(),
        })
    }

    /// This person's standing toward app `app_id` of org `org_id`.
    pub fn standing(&self, org_id: Uuid, app_id: Uuid) -> Standing {
        Standing {
            member: self.member_orgs.contains(&org_id),
            active_crew: self.crew_orgs.contains(&org_id),
            granted: self.granted_apps.contains(&app_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_and_granted_active_crew_are_admitted_and_nobody_else() {
        let s = |member, active_crew, granted| {
            Standing {
                member,
                active_crew,
                granted,
            }
            .admits()
        };
        assert!(s(true, false, false), "a member");
        assert!(s(true, true, false), "a member who is also crew");
        assert!(s(false, true, true), "active crew holding a grant");
        assert!(!s(false, true, false), "active crew without a grant");
        assert!(!s(false, false, true), "a grant without being active crew");
        assert!(!s(false, false, false), "nobody");
    }

    #[test]
    fn a_persons_standing_is_read_per_org_and_per_app() {
        let (org, other_org, app, other_app) = (
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
        );
        let reach = PersonReach {
            member_orgs: HashSet::new(),
            crew_orgs: [org].into(),
            granted_apps: [app].into(),
        };
        assert!(reach.standing(org, app).admits());
        assert!(!reach.standing(org, other_app).admits(), "no grant on it");
        assert!(
            !reach.standing(other_org, app).admits(),
            "crew of another org only"
        );
    }
}
