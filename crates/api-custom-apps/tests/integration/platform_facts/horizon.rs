//! `lastActivityAt` looks back `LAST_ACTIVITY_HORIZON_DAYS` (180) and no
//! further. Each of its three probes skips staff's and unattended rows, so
//! without a floor a churned customer whose webhooks keep arriving makes every
//! build walk every invocation since their last attributed one. Past the
//! horizon the customer reads as inactive: `null`.

use chrono::{DateTime, Duration, Utc};
use entity::{app_function_invocations, custom_app_view_event};
use oxy_api_custom_apps::custom_apps_functions::host::platform_facts::LAST_ACTIVITY_HORIZON_DAYS;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use uuid::Uuid;

use super::world::{World, at, now, org};

/// The three kinds of customer activity `lastActivityAt` reads.
#[derive(Clone, Copy, Debug)]
enum Kind {
    View,
    Call,
    Thread,
}

impl World {
    /// A customer org whose one customer activity is a `kind` at `when`.
    async fn active_once(&self, kind: Kind, when: DateTime<Utc>) -> Uuid {
        let db = &self.db;
        let org = World::org(db, "Quiet").await;
        let ws = World::workspace(db, org, when).await;
        let person = World::person(db, when, &[org]).await;
        let (app, build) = World::app(db, org, ws, "Quiet", None).await;
        match kind {
            Kind::View => self.production_view(app, person, when).await,
            Kind::Call => self.production_call(app, build, person, when).await,
            Kind::Thread => self.thread(ws, Some(person), when).await,
        }
        org
    }

    async fn production_view(&self, app: Uuid, user: Uuid, when: DateTime<Utc>) {
        custom_app_view_event::ActiveModel {
            id: Set(Uuid::new_v4()),
            app_id: Set(app),
            user_id: Set(user),
            user_email: Set("quiet@facts.test".into()),
            session_id: Set(Uuid::new_v4()),
            viewed_at: Set(when.fixed_offset()),
            user_agent_class: Set("browser".into()),
            source: Set("subpath".into()),
            environment: Set("production".into()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed view");
    }

    /// An invocation the customer made themselves.
    async fn production_call(&self, app: Uuid, build: Uuid, user: Uuid, when: DateTime<Utc>) {
        app_function_invocations::ActiveModel {
            id: Set(Uuid::new_v4()),
            app_id: Set(app),
            build_id: Set(build),
            user_id: Set(Some(user)),
            function_name: Set("sync".into()),
            mode: Set("route".into()),
            status: Set("ok".into()),
            created_at: Set(when.fixed_offset()),
            environment: Set("production".into()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed invocation");
    }
}

/// Per kind, one customer an hour past the horizon and one an hour inside it:
/// the first reads `null`, the second its activity. Every probe is floored,
/// not just one.
#[tokio::test]
async fn last_activity_older_than_the_horizon_is_null_and_newer_is_returned() {
    let w = World::new().await;
    let horizon = now() - Duration::days(LAST_ACTIVITY_HORIZON_DAYS);
    let (outside, inside) = (horizon - Duration::hours(1), horizon + Duration::hours(1));
    let mut cases = Vec::new();
    for kind in [Kind::View, Kind::Call, Kind::Thread] {
        cases.push((kind, w.active_once(kind, outside).await, None));
        cases.push((kind, w.active_once(kind, inside).await, Some(inside)));
    }
    let facts = w.facts().await;
    for (kind, id, expected) in cases {
        let quiet = org(&facts, id);
        assert_eq!(
            quiet.last_activity_at,
            expected,
            "a customer whose only activity is a {kind:?} at {}",
            expected.unwrap_or(outside)
        );
        if let Kind::View = kind {
            assert_eq!(
                quiet.apps[0].customer_last_active_at, expected,
                "the app's customer last use is floored the same"
            );
        }
    }
    assert_eq!(
        org(&facts, w.acme).last_activity_at,
        Some(at(10, 13, 11)),
        "activity well inside the horizon is untouched"
    );
}
