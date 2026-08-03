import { ActivityEvents } from "./components/ActivityEvents";
import { ActivitySummary } from "./components/ActivitySummary";
import { ActivityVisitors } from "./components/ActivityVisitors";

/**
 * Activity tab — three stacked sections, each owning its own query:
 *
 *   1. `ActivitySummary`  — last viewed / 7d uniques / 7d views / 7d events.
 *   2. `ActivityVisitors` — per-user roll-up over the last 7 days.
 *   3. `ActivityEvents`   — custom events by name, drillable to occurrences.
 *
 * Storage is PostgreSQL; queries are app-admin gated. See
 * `custom_apps_activity` on the backend for the SQL.
 */
export const Activity = ({ appId }: { appId: string }) => (
  <div className='space-y-4 p-4 pt-0' data-testid='admin-app-activity'>
    <ActivitySummary appId={appId} />
    <ActivityVisitors appId={appId} />
    <ActivityEvents appId={appId} />
  </div>
);
