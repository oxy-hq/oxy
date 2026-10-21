//! The world `platform_facts` reads: three orgs and a partner, seeded on a
//! database of its own, and the one call every case makes.

use crate::common::{Schema, fresh_db};
use chrono::{DateTime, TimeZone, Utc};
use entity::{
    app_builds, app_environment_events, app_function_invocations, apps, custom_app_view_event,
    org_billing, org_members, organizations, partner_grants, partner_orgs, threads, users,
    workspaces,
};
use oxy_api_custom_apps::custom_apps_functions::host::platform_facts::{
    OxyPlatformFacts, OxyPlatformOrg, build_platform_facts,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection, DbBackend, Statement,
};
use serde_json::json;
use uuid::Uuid;

pub fn at(m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap()
}

pub fn now() -> DateTime<Utc> {
    at(10, 14, 5)
}

pub struct World {
    pub db: DatabaseConnection,
    pub staff_org: Uuid,
    pub acme: Uuid,
    pub beta: Uuid,
    pub partner: Uuid,
    pub ops: Uuid,
    pub board: Uuid,
    /// The build Ops's invocations hang off.
    build: Uuid,
}

impl World {
    pub(super) async fn org(db: &DatabaseConnection, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        organizations::ActiveModel {
            id: Set(id),
            name: Set(name.into()),
            slug: Set(format!("facts-{}", id.simple())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed org");
        id
    }

    /// A user signed in at `last_login`, member of each of `orgs`.
    pub(super) async fn person(
        db: &DatabaseConnection,
        last_login: DateTime<Utc>,
        orgs: &[Uuid],
    ) -> Uuid {
        let id = Uuid::new_v4();
        users::ActiveModel {
            id: Set(id),
            email: Set(Some(format!("{}@facts.test", id.simple()))),
            name: Set("Someone".into()),
            email_verified: Set(true),
            status: Set(users::UserStatus::Active),
            last_login_at: Set(last_login.fixed_offset()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed user");
        for org in orgs {
            org_members::ActiveModel {
                id: Set(Uuid::new_v4()),
                org_id: Set(*org),
                user_id: Set(id),
                role: Set(org_members::OrgRole::Member),
                ..Default::default()
            }
            .insert(db)
            .await
            .expect("seed member");
        }
        id
    }

    pub(super) async fn workspace(
        db: &DatabaseConnection,
        org: Uuid,
        created: DateTime<Utc>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        workspaces::ActiveModel {
            id: Set(id),
            name: Set("facts".into()),
            org_id: Set(Some(org)),
            created_at: Set(created.fixed_offset()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed workspace");
        id
    }

    /// An app and the build its invocations hang off.
    pub(super) async fn app(
        db: &DatabaseConnection,
        org: Uuid,
        ws: Uuid,
        name: &str,
        published: Option<DateTime<Utc>>,
    ) -> (Uuid, Uuid) {
        let id = Uuid::new_v4();
        apps::ActiveModel {
            id: Set(id),
            org_id: Set(org),
            project_id: Set(ws),
            slug: Set(format!("{}-{}", name.to_lowercase(), id.simple())),
            name: Set(name.into()),
            branch: Set("main".into()),
            source_repo: Set("facts/app".into()),
            status: Set("active".into()),
            source_type: Set("s3".into()),
            source_config: Set(json!({})),
            visibility: Set("org".into()),
            published_at: Set(published.map(|p| p.fixed_offset())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed app");
        let build = Uuid::new_v4();
        app_builds::ActiveModel {
            id: Set(build),
            app_id: Set(id),
            build_id: Set("b1".into()),
            s3_prefix: Set("apps/b1".into()),
            created_at: Set(at(9, 1, 0).fixed_offset()),
            validation_status: Set("ok".into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed build");
        (id, build)
    }

    async fn view(&self, user: Uuid, when: DateTime<Utc>, env: &str) {
        custom_app_view_event::ActiveModel {
            id: Set(Uuid::new_v4()),
            app_id: Set(self.ops),
            user_id: Set(user),
            user_email: Set("someone@facts.test".into()),
            session_id: Set(Uuid::new_v4()),
            viewed_at: Set(when.fixed_offset()),
            user_agent_class: Set("browser".into()),
            source: Set("subpath".into()),
            environment: Set(env.into()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed view");
    }

    /// An invocation of Ops; `user` is `None` for an unattended one (a
    /// schedule, a webhook).
    async fn call(&self, status: &str, when: DateTime<Utc>, env: &str, user: Option<Uuid>) {
        app_function_invocations::ActiveModel {
            id: Set(Uuid::new_v4()),
            app_id: Set(self.ops),
            build_id: Set(self.build),
            user_id: Set(user),
            function_name: Set("sync".into()),
            mode: Set("route".into()),
            status: Set(status.into()),
            created_at: Set(when.fixed_offset()),
            environment: Set(env.into()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed invocation");
    }

    async fn release(&self, env: &str, when: DateTime<Utc>) {
        app_environment_events::ActiveModel {
            id: Set(Uuid::new_v4()),
            app_id: Set(self.ops),
            environment: Set(env.into()),
            build_id: Set(None),
            action: Set("promote".into()),
            actor: Set(None),
            at: Set(when.fixed_offset()),
        }
        .insert(&self.db)
        .await
        .expect("seed release");
    }

    /// A thread; `user` is `None` for one nobody is attributed to.
    pub(super) async fn thread(&self, ws: Uuid, user: Option<Uuid>, when: DateTime<Utc>) {
        threads::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user),
            title: Set("t".into()),
            input: Set("q".into()),
            output: Set(String::new()),
            source: Set("web".into()),
            source_type: Set("analytics".into()),
            references: Set("[]".into()),
            is_processing: Set(false),
            created_at: Set(when.fixed_offset()),
            project_id: Set(ws),
            sandbox_info: Set(None),
        }
        .insert(&self.db)
        .await
        .expect("seed thread");
    }

    /// A run on 10-06; returns its id.
    async fn run(&self, ws: Uuid, status: &str, source: &str, parent: Option<&str>) -> String {
        let id = Uuid::new_v4().to_string();
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO agentic_runs (id, workspace_id, question, task_status, source_type, \
                 parent_run_id, attempt, created_at, updated_at) \
                 VALUES ($1, $2, '', $3, $4, $5, 0, $6, $6)",
                [
                    id.clone().into(),
                    ws.into(),
                    status.into(),
                    source.into(),
                    parent.map(str::to_string).into(),
                    at(10, 6, 9).into(),
                ],
            ))
            .await
            .expect("seed run");
        id
    }

    /// Partner Co manages Beta, which is past due.
    async fn beta_is_managed_and_past_due(&self) {
        partner_grants::ActiveModel {
            org_id: Set(self.partner),
            status: Set("active".into()),
            created_by: Set(None),
            created_at: sea_orm::ActiveValue::NotSet,
        }
        .insert(&self.db)
        .await
        .expect("seed partner grant");
        partner_orgs::ActiveModel {
            id: Set(Uuid::new_v4()),
            partner_org_id: Set(self.partner),
            managed_org_id: Set(self.beta),
            created_by: Set(None),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed partner link");
        org_billing::ActiveModel {
            id: Set(Uuid::new_v4()),
            org_id: Set(self.beta),
            status: Set(org_billing::BillingStatus::PastDue),
            seats_paid: Set(5),
            grace_period_ends_at: Set(Some(at(10, 20, 0).fixed_offset())),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .expect("seed billing");
    }

    pub async fn new() -> Self {
        // `Schema::All`: `agentic_runs` is the agentic runtime's table.
        let db = fresh_db(Schema::All).await.0;
        let staff_org = Self::org(&db, "Oxygen").await;
        let acme = Self::org(&db, "Acme").await;
        let beta = Self::org(&db, "Beta").await;
        let partner = Self::org(&db, "Partner Co").await;
        let ws = Self::workspace(&db, acme, at(9, 1, 0)).await;
        // Re-publishing bumps `published_at`; the first promote is older.
        let (ops, build) = Self::app(&db, acme, ws, "Ops", Some(at(10, 6, 8))).await;
        let (board, _) = Self::app(&db, acme, ws, "Board", None).await;
        let world = Self {
            db,
            staff_org,
            acme,
            beta,
            partner,
            ops,
            board,
            build,
        };
        world.seed_acme(ws).await;
        world.beta_is_managed_and_past_due().await;
        Self::person(&world.db, at(10, 1, 0), &[beta]).await;
        world
    }

    async fn seed_acme(&self, ws: Uuid) {
        let db = &self.db;
        let alice = Self::person(db, at(10, 13, 8), &[self.acme]).await;
        let bob = Self::person(db, at(10, 1, 0), &[self.acme]).await;
        let dan = Self::person(db, at(10, 2, 0), &[self.acme]).await;
        // Staff: a member of the internal org, and of the customer's.
        let sam = Self::person(db, at(10, 14, 2), &[self.staff_org, self.acme]).await;

        self.view(alice, at(9, 29, 10), "production").await; // W0
        self.view(alice, at(10, 6, 10), "production").await; // W1
        self.view(sam, at(10, 6, 11), "production").await; // W1, staff
        self.view(bob, at(10, 8, 9), "production").await; // W1 and last7d
        self.view(alice, at(10, 13, 9), "production").await; // last7d
        // Staff opening the app after the customer last did: the app's last
        // open, never the customer's activity.
        self.view(sam, at(10, 14, 2), "production").await;
        self.view(alice, at(10, 6, 12), "staging").await;
        self.view(alice, at(10, 14, 4), "staging").await; // later than any production view

        let (alice, sam, dan) = (Some(alice), Some(sam), Some(dan));
        self.thread(ws, alice, at(9, 15, 10)).await; // before the windows
        self.thread(ws, alice, at(10, 6, 13)).await;
        self.thread(ws, sam, at(10, 6, 14)).await;
        self.thread(ws, dan, at(10, 6, 15)).await;
        self.thread(ws, dan, at(10, 12, 10)).await; // last7d
        self.thread(ws, None, at(10, 13, 12)).await; // last7d, nobody's

        self.call("ok", at(10, 6, 10), "production", None).await;
        self.call("error", at(10, 6, 11), "production", None).await;
        self.call("ok", at(10, 6, 12), "staging", None).await;
        self.call("error", at(10, 6, 13), "staging", None).await;
        // The customer's last activity: bob calling a function himself.
        self.call("ok", at(10, 13, 11), "production", Some(bob))
            .await;
        // An unattended run after it.
        self.call("ok", at(10, 14, 3), "production", None).await;

        self.release("production", at(9, 3, 0)).await; // the first publish
        self.release("production", at(10, 6, 8)).await;
        self.release("staging", at(10, 6, 7)).await;

        let parent = self.run(ws, "done", "analytics", None).await;
        self.run(ws, "failed", "analytics", None).await;
        self.run(ws, "failed", "preagg_cycle", None).await; // a platform daemon
        self.run(ws, "done", "analytics", Some(&parent)).await; // a delegated child
    }

    pub async fn facts(&self) -> OxyPlatformFacts {
        self.facts_for(2).await
    }

    pub async fn facts_for(&self, weeks: u32) -> OxyPlatformFacts {
        build_platform_facts(&self.db, &[self.staff_org], weeks, now())
            .await
            .expect("platform facts")
    }
}

pub fn org(facts: &OxyPlatformFacts, id: Uuid) -> &OxyPlatformOrg {
    facts
        .orgs
        .iter()
        .find(|o| o.id == id)
        .unwrap_or_else(|| panic!("org {id} missing from the facts"))
}
