//! The per-app reads: each app's last open by anyone, the customer's own last
//! open, and how many of the customer's people opened it in `last7d`.
//!
//! All three are production views only, and the two customer reads leave out
//! staff (members of an internal org) exactly as every `*People` count does.

use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use uuid::Uuid;

// Each app's last production open, staff's included — the app's "last active"
// as the admin apps list shows it. One probe of `(app_id, viewed_at DESC)` per
// app (`custom_apps_activity::last_active_at_by_app` answers the same with a
// `MAX` over every view, which the admin list's page of apps can afford).
pub(super) const LAST_VIEWED_SQL: &str = "\
SELECT a.id AS app_id, last.viewed_at AS at \
FROM apps a \
CROSS JOIN LATERAL (SELECT v.viewed_at FROM custom_app_view_event v \
                    WHERE v.app_id = a.id AND v.environment = 'production' \
                    ORDER BY v.viewed_at DESC LIMIT 1) last";

// $1 the internal orgs, $2 the horizon's start (`LAST_ACTIVITY_HORIZON_DAYS`
// before now). Each app's latest production view by a non-staff person since
// $2: the customer's own last use. The same probe as `LAST_VIEWED_SQL`, skipping
// staff's rows, so it is floored at $2 like every customer-activity probe: an
// app only staff open would otherwise walk every view staff ever made of it.
// It is also the app-view half of the org's `lastActivityAt`.
pub(super) const CUSTOMER_LAST_VIEWED_SQL: &str = "\
WITH staff AS (SELECT DISTINCT user_id FROM org_members WHERE org_id = ANY($1::uuid[])) \
SELECT a.id AS app_id, lv.at \
FROM apps a \
CROSS JOIN LATERAL (SELECT v.viewed_at AS at FROM custom_app_view_event v \
                    WHERE v.app_id = a.id AND v.environment = 'production' \
                      AND v.viewed_at >= $2 \
                      AND NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = v.user_id) \
                    ORDER BY v.viewed_at DESC LIMIT 1) lv";

// $1 the internal orgs, $2 every app, $3 and $4 `last7d`'s window. Each app's
// distinct non-staff people with a production view in it. Named apps and a
// bounded time, like the windowed reads, so it rides `(app_id, viewed_at)`.
pub(super) const PEOPLE_7D_SQL: &str = "\
WITH staff AS (SELECT DISTINCT user_id FROM org_members WHERE org_id = ANY($1::uuid[])) \
SELECT v.app_id, COUNT(DISTINCT v.user_id)::bigint AS people \
FROM custom_app_view_event v \
WHERE v.app_id = ANY($2) AND v.environment = 'production' \
  AND v.viewed_at >= $3 AND v.viewed_at < $4 \
  AND NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = v.user_id) \
GROUP BY v.app_id";

/// An instant per app (a last view).
#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct AppAtRow {
    pub app_id: Uuid,
    pub at: DateTime<Utc>,
}

/// A count per app (its people in `last7d`).
#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct AppCountRow {
    pub app_id: Uuid,
    pub people: i64,
}
