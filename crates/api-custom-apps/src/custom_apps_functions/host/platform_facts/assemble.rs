//! The rows the queries returned, shaped into the contract: one
//! [`OxyPlatformOrg`] per org, by name.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::app_use::{AppAtRow, AppCountRow};
use super::llm_cost::{self, LlmCostRow};
use super::queries::{AppRow, BillingRow, OrgAtRow, OrgRow, Sources, WindowRow};
use super::{
    OxyPlatformApp, OxyPlatformBilling, OxyPlatformMilestones, OxyPlatformOrg, OxyPlatformWeek,
};

fn n(count: i64) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

/// Each org's windows: `windows` entries, the last of them `last7d`.
type Tallies = HashMap<Uuid, Vec<OxyPlatformWeek>>;

/// A row about one org in one window.
trait InWindow {
    fn org_id(&self) -> Uuid;
    fn idx(&self) -> i64;
}

impl InWindow for WindowRow {
    fn org_id(&self) -> Uuid {
        self.org_id
    }
    fn idx(&self) -> i64 {
        self.idx
    }
}

impl InWindow for LlmCostRow {
    fn org_id(&self) -> Uuid {
        self.org_id
    }
    fn idx(&self) -> i64 {
        self.idx
    }
}

/// Adds one source's rows to `tallies` through `put`. A row whose window is
/// out of range is dropped rather than trusted.
fn tally<R: InWindow>(
    tallies: &mut Tallies,
    windows: usize,
    rows: &[R],
    put: impl Fn(&mut OxyPlatformWeek, &R),
) {
    for row in rows {
        let Some(idx) = usize::try_from(row.idx()).ok().filter(|i| *i < windows) else {
            continue;
        };
        let slots = tallies
            .entry(row.org_id())
            .or_insert_with(|| vec![OxyPlatformWeek::default(); windows]);
        put(&mut slots[idx], row);
    }
}

fn tallies(src: &Sources, windows: usize) -> Tallies {
    let mut t = Tallies::new();
    tally(&mut t, windows, &src.views, |w, r| {
        w.app_views = n(r.events);
        w.app_people = n(r.people);
    });
    tally(&mut t, windows, &src.threads, |w, r| {
        w.threads = n(r.events);
        w.chat_people = n(r.people);
    });
    tally(&mut t, windows, &src.active, |w, r| {
        w.active_people = n(r.people)
    });
    tally(&mut t, windows, &src.runs, |w, r| {
        w.agent_runs = n(r.events);
        w.agent_run_failures = n(r.failures);
    });
    tally(&mut t, windows, &src.calls, |w, r| {
        w.function_calls = n(r.events);
        w.function_failures = n(r.failures);
    });
    tally(&mut t, windows, &src.releases, |w, r| {
        w.releases = n(r.events)
    });
    // One row per model: each adds to the window.
    tally(&mut t, windows, &src.llm, llm_cost::add);
    t
}

/// Everything about an org that is not a windowed count, keyed by org.
struct Lookup {
    billing: HashMap<Uuid, BillingRow>,
    partners: HashMap<Uuid, Uuid>,
    sign_ins: HashMap<Uuid, DateTime<Utc>>,
    first_publishes: HashMap<Uuid, DateTime<Utc>>,
    first_threads: HashMap<Uuid, DateTime<Utc>>,
    apps: HashMap<Uuid, Vec<OxyPlatformApp>>,
    /// The latest customer activity (the queries decide what counts).
    last_activity: HashMap<Uuid, DateTime<Utc>>,
}

/// One instant per org; an org whose instant is null is left out.
fn instants(rows: Vec<OrgAtRow>) -> HashMap<Uuid, DateTime<Utc>> {
    rows.into_iter()
        .filter_map(|r| Some((r.org_id, r.at?)))
        .collect()
}

/// What the per-app reads said, keyed by app.
struct AppUse {
    last_viewed: HashMap<Uuid, DateTime<Utc>>,
    customer_last_viewed: HashMap<Uuid, DateTime<Utc>>,
    people7d: HashMap<Uuid, u64>,
}

impl AppUse {
    fn new(last: Vec<AppAtRow>, customer: Vec<AppAtRow>, people: Vec<AppCountRow>) -> Self {
        let by_app = |rows: Vec<AppAtRow>| -> HashMap<Uuid, DateTime<Utc>> {
            rows.into_iter().map(|r| (r.app_id, r.at)).collect()
        };
        Self {
            last_viewed: by_app(last),
            customer_last_viewed: by_app(customer),
            people7d: people
                .into_iter()
                .map(|r| (r.app_id, n(r.people)))
                .collect(),
        }
    }
}

/// Each org's latest customer activity: the newer of its non-view activity
/// (`rows`) and the customer's last view of any of its apps.
fn last_activity(
    rows: Vec<OrgAtRow>,
    apps: &HashMap<Uuid, Vec<OxyPlatformApp>>,
) -> HashMap<Uuid, DateTime<Utc>> {
    let mut latest = instants(rows);
    for (org, list) in apps {
        for at in list.iter().filter_map(|a| a.customer_last_active_at) {
            latest
                .entry(*org)
                .and_modify(|seen| *seen = (*seen).max(at))
                .or_insert(at);
        }
    }
    latest
}

impl Lookup {
    fn new(src: Sources) -> Self {
        let app_use = AppUse::new(src.last_viewed, src.customer_last_viewed, src.people7d);
        let apps = apps_by_org(src.apps, &app_use);
        Self {
            billing: src.billing.into_iter().map(|b| (b.org_id, b)).collect(),
            partners: src
                .partners
                .into_iter()
                .map(|p| (p.managed_org_id, p.partner_org_id))
                .collect(),
            sign_ins: instants(src.sign_ins),
            first_publishes: instants(src.first_publishes),
            first_threads: instants(src.first_threads),
            last_activity: last_activity(src.last_activity, &apps),
            apps,
        }
    }

    fn billing(&self, org: Uuid) -> OxyPlatformBilling {
        self.billing
            .get(&org)
            .map_or_else(OxyPlatformBilling::default, |b| OxyPlatformBilling {
                status: Some(b.status.clone()),
                seats_paid: Some(i64::from(b.seats_paid)),
                grace_ends_at: b.grace_period_ends_at,
            })
    }
}

/// Each org's apps, by name, with their production use.
fn apps_by_org(apps: Vec<AppRow>, used: &AppUse) -> HashMap<Uuid, Vec<OxyPlatformApp>> {
    let mut by_org: HashMap<Uuid, Vec<OxyPlatformApp>> = HashMap::new();
    for app in apps {
        by_org.entry(app.org_id).or_default().push(OxyPlatformApp {
            id: app.id,
            last_active_at: used.last_viewed.get(&app.id).copied(),
            customer_last_active_at: used.customer_last_viewed.get(&app.id).copied(),
            people7d: used.people7d.get(&app.id).copied().unwrap_or(0),
            slug: app.slug,
            name: app.name,
            published_at: app.published_at,
        });
    }
    for list in by_org.values_mut() {
        list.sort_by(|a, b| by_name((&a.name, a.id), (&b.name, b.id)));
    }
    by_org
}

/// Case-insensitive by name, then exactly, then by id: a stable order.
fn by_name(a: (&str, Uuid), b: (&str, Uuid)) -> std::cmp::Ordering {
    a.0.to_lowercase()
        .cmp(&b.0.to_lowercase())
        .then_with(|| a.0.cmp(b.0))
        .then_with(|| a.1.cmp(&b.1))
}

/// Every org in `src`, by name. `windows` counts the completed weeks plus
/// `last7d`; `internal` lists the internal orgs.
pub(super) fn orgs(mut src: Sources, internal: &[Uuid], windows: usize) -> Vec<OxyPlatformOrg> {
    let mut tallies = tallies(&src, windows);
    let rows = std::mem::take(&mut src.orgs);
    let mut lookup = Lookup::new(src);
    let mut orgs: Vec<OxyPlatformOrg> = rows
        .into_iter()
        .map(|row| {
            let counts = tallies
                .remove(&row.id)
                .unwrap_or_else(|| vec![OxyPlatformWeek::default(); windows]);
            let is_internal = internal.contains(&row.id);
            org(row, &mut lookup, is_internal, counts)
        })
        .collect();
    orgs.sort_by(|a, b| by_name((&a.name, a.id), (&b.name, b.id)));
    orgs
}

/// One org. `counts` holds its completed weeks, then `last7d`.
fn org(
    row: OrgRow,
    lookup: &mut Lookup,
    internal: bool,
    mut counts: Vec<OxyPlatformWeek>,
) -> OxyPlatformOrg {
    let id = row.id;
    let last7d = counts.pop().unwrap_or_default();
    OxyPlatformOrg {
        id,
        slug: row.slug,
        name: row.name,
        created_at: row.created_at,
        internal,
        partner_org_id: lookup.partners.get(&id).copied(),
        members: n(row.members),
        workspaces: n(row.workspaces),
        billing: lookup.billing(id),
        last_sign_in_at: lookup.sign_ins.get(&id).copied(),
        last_activity_at: lookup.last_activity.get(&id).copied(),
        apps: lookup.apps.remove(&id).unwrap_or_default(),
        milestones: OxyPlatformMilestones {
            first_workspace_at: row.first_workspace_at,
            first_app_published_at: lookup.first_publishes.get(&id).copied(),
            first_thread_at: lookup.first_threads.get(&id).copied(),
        },
        weekly: counts,
        last7d,
    }
}
