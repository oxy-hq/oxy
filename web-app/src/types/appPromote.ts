/**
 * The plan a production move shows before it acts and answers after
 * (`POST /customer-apps/{id}/environments/{to}/promote|rollback`,
 * `custom_apps_promote::plan::Plan`). The same JSON `oxyc promote --dry-run`
 * prints, so the console and the CLI read one answer.
 */
export interface MovePlan {
  action: "promote" | "rollback";
  app_id: string;
  from: string | null;
  to: string;
  build: { id: string; build_id: string; published_by: string | null; created_at: string };
  current: { id: string; build_id: string } | null;
  /** What `to` serves now, sent back by the act (`null`: nothing). */
  expect_current: string | null;
  served_since: string | null;
  functions: { added: string[]; removed: string[]; changed: string[] };
  /** Production only. */
  schedules: {
    added: MoveCadence[];
    removed: MoveCadence[];
    changed: { function: string; from: MoveCadence; to: MoveCadence }[];
  } | null;
  migrations: { when: "before_move" | "queued_after"; oltp: string[]; airhouse: string[] };
  required_secrets_unset: string[];
  blockers: { code: string; message: string }[];
  warnings: string[];
  dry_run: boolean;
  moved: boolean;
}

export interface MoveCadence {
  function: string;
  cron: string;
  timezone: string;
}

/** The blocker a confirm clears by sending `confirm_schedule_removals`. */
export const SCHEDULE_REMOVAL_UNCONFIRMED = "schedule_removal_unconfirmed";

/** Which production move a plan is for: the draft promoted from staging, or a
 *  rollback to a retained build. `buildId` is the `app_builds.id`. */
export interface MoveTarget {
  act: "promote" | "rollback";
  buildId: string;
}

/** The body of a plan or act request. */
export interface MoveRequest {
  dry_run?: boolean;
  confirm_schedule_removals?: string[];
  expect_current?: string | null;
}
