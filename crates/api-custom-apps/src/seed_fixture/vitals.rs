//! Production's Web Vitals before the seed: deterministic `custom_app_event`
//! rows, two per page load — `oxy-app-ready` and `oxy-web-vitals` — as the
//! client runtime sends them (`client/runtime.js`).
//!
//! The beacon stamps every event with the server's clock, so no real path can
//! produce a past page load; these rows are the one writer of seeded vitals,
//! and the only thing to change when `custom_app_event` moves to Airhouse.
//!
//! Each vitals payload carries `path` (the screen) and `build` (the
//! `app_builds.id` the page ran — what `oxy-app-ready.build` sends), so a
//! reader can attribute it exactly. Two screens, `/` and `/store` (the store
//! id rides the query string, which the runtime never records), each with at
//! least [`MIN_SESSIONS`] page loads per stretch. r4 regresses `/store`: its
//! LCP and INP p75 roughly double.

use chrono::{DateTime, Duration, Utc};
use entity::custom_app_event::{Column, Entity};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::Set, ConnectionTrait, DbErr, EntityTrait};
use serde_json::{Value, json};
use uuid::Uuid;

use super::calendar::{Calendar, LIVE, Stretch};
use super::history::splitmix64;
use crate::custom_apps_tracking::build_event_model;

/// The screens the page has.
pub const SCREENS: [&str; 2] = ["/", "/store"];

/// Page loads per screen per stretch, at the least. The release timeline
/// compares a screen only above its own sample floor; this clears it with room.
pub const MIN_SESSIONS: usize = 36;

/// The keys `runtime.js` puts in an `oxy-web-vitals` payload, plus the two
/// attribution keys.
pub const VITALS_KEYS: [&str; 8] = ["ttfb", "dcl", "fcp", "lcp", "cls", "inp", "path", "build"];

const CHUNK: usize = 500;

/// One seeded page load.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLoad {
    pub session: Uuid,
    pub release: usize,
    pub screen: &'static str,
    pub at: DateTime<Utc>,
    pub vitals: Value,
}

/// Every seeded page load, oldest stretch first. `builds[k]` is release
/// `k + 1`'s `app_builds.id`.
pub fn page_loads(app_id: Uuid, cal: &Calendar, builds: &[Uuid]) -> Vec<PageLoad> {
    let mut out = Vec::new();
    for s in cal.stretches() {
        for (k, screen) in SCREENS.iter().enumerate() {
            out.extend(screen_loads(app_id, &s, k, screen, builds[s.release - 1]));
        }
    }
    out
}

/// How many loads a screen gets in `s`: two an hour, never under the floor.
pub fn sessions_in(s: &Stretch) -> usize {
    ((s.hours() * 2.0).ceil() as usize).max(MIN_SESSIONS)
}

fn screen_loads(
    app_id: Uuid,
    s: &Stretch,
    k: usize,
    screen: &'static str,
    build: Uuid,
) -> Vec<PageLoad> {
    let n = sessions_in(s);
    let span = (s.end - s.start).num_seconds();
    (0..n)
        .map(|i| {
            // Spread evenly, the two screens interleaved, all inside the stretch.
            let at =
                s.start + Duration::seconds(span * (2 * i + k + 1) as i64 / (2 * n + 2) as i64);
            let session = Uuid::new_v5(
                &app_id,
                format!("session/r{}/{screen}/{i}", s.release).as_bytes(),
            );
            let jitter = |key: &str, range: u64| {
                let seed = Uuid::new_v5(&session, key.as_bytes()).as_u128() as u64;
                (splitmix64(seed) % range) as i64
            };
            let slow = s.release == LIVE && screen == "/store";
            let ttfb = 80 + jitter("ttfb", 120);
            let fcp = 420 + jitter("fcp", 380);
            let lcp = if slow {
                2300 + jitter("lcp", 900)
            } else {
                fcp + 500 + jitter("lcp", 500)
            };
            let inp = if slow {
                190 + jitter("inp", 140)
            } else {
                56 + jitter("inp", 80)
            };
            let vitals = json!({
                "ttfb": ttfb,
                "dcl": fcp - 40,
                "fcp": fcp,
                "lcp": lcp,
                "cls": jitter("cls", 60),
                "inp": inp,
                "path": screen,
                "build": build.to_string(),
            });
            PageLoad {
                session,
                release: s.release,
                screen,
                at,
                vitals,
            }
        })
        .collect()
}

/// Write two events per load, as `viewer`. A row that already exists is left
/// alone. Answers how many events were written.
pub async fn write_events<C: ConnectionTrait>(
    conn: &C,
    app_id: Uuid,
    viewer: (Uuid, &str),
    builds: &[Uuid],
    loads: &[PageLoad],
) -> Result<usize, DbErr> {
    let mut models = Vec::with_capacity(loads.len() * 2);
    for load in loads {
        let build = builds[load.release - 1].to_string();
        let ready = json!({ "how": "auto", "build": build });
        models.push(event(
            app_id,
            viewer,
            load,
            "oxy-app-ready",
            ready,
            load.at,
        )?);
        let vitals_at = load.at + Duration::seconds(10);
        models.push(event(
            app_id,
            viewer,
            load,
            "oxy-web-vitals",
            load.vitals.clone(),
            vitals_at,
        )?);
    }
    let written = models.len();
    let mut models = models.into_iter().peekable();
    while models.peek().is_some() {
        let chunk: Vec<_> = models.by_ref().take(CHUNK).collect();
        Entity::insert_many(chunk)
            .on_conflict(OnConflict::column(Column::Id).do_nothing().to_owned())
            .exec_without_returning(conn)
            .await?;
    }
    Ok(written)
}

fn event(
    app_id: Uuid,
    (user_id, email): (Uuid, &str),
    load: &PageLoad,
    name: &str,
    payload: Value,
    at: DateTime<Utc>,
) -> Result<entity::custom_app_event::ActiveModel, DbErr> {
    let mut model = build_event_model(
        app_id,
        user_id,
        email.to_string(),
        load.session,
        name.to_string(),
        payload,
        "production",
        at.fixed_offset(),
    )
    .map_err(|e| DbErr::Custom(format!("seeded event refused: {e}")))?;
    // Deterministic, so a re-run inserts nothing twice.
    model.id = Set(event_id(load.session, name));
    Ok(model)
}

/// The deterministic id of one session's event.
fn event_id(session: Uuid, name: &str) -> Uuid {
    Uuid::new_v5(&session, name.as_bytes())
}

/// The id of the LAST event [`write_events`] writes: the newest stretch's last
/// page load's `oxy-web-vitals`. The history is written invocations first and
/// events last, in order, so this row exists only once everything before it
/// does — the fixture's "fully written" sentinel. It depends on the stretch
/// lengths, which the calendar fixes, and not on the seed time.
pub fn sentinel_event_id(app_id: Uuid) -> Uuid {
    let cal = Calendar::at(chrono::Utc::now());
    let builds = vec![Uuid::nil(); super::RELEASES];
    let last = page_loads(app_id, &cal, &builds)
        .pop()
        .expect("every stretch has page loads");
    event_id(last.session, "oxy-web-vitals")
}
