import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DEFAULT_RELEASE_WINDOW,
  RELEASE_WINDOWS,
  type ReleaseWindow,
  useAppReleases
} from "@/hooks/api/customApps/useAppReleases";
import { AdminAsync } from "@/pages/admin/components/AdminAsync";
import type { MoveTarget } from "@/types/appPromote";
import type { Release, ReleaseTimeline } from "@/types/appReleases";
import type { CustomApp } from "@/types/apps";
import { LIFECYCLE_COPY, PlannedMoveConfirm } from "../../../AppLifecycle";
import type { ConfirmCopy } from "../../../AppLifecycle/lifecycleCopy";
import { ReleaseLanes } from "./components/ReleaseLanes";
import { ReleaseRow } from "./components/ReleaseRow";

/** A rollback the operator asked for: the move and the words its confirm says. */
interface AskedRollback {
  target: MoveTarget;
  copy: ConfirmCopy;
}

/**
 * The release timeline — production, staging and sandboxes over one window, and
 * production's releases, each with its failure rate, route p95, Web Vitals per
 * screen and migrations beside the release before it. A red release serving now is
 * rolled back from right here, on the same dry-run plan the lifecycle card confirms.
 */
export const Releases = ({
  app,
  onOpenFunction
}: {
  app: CustomApp;
  onOpenFunction: (name: string) => void;
}) => {
  const [days, setDays] = useState<ReleaseWindow>(DEFAULT_RELEASE_WINDOW);
  const releases = useAppReleases(app.id, days);
  const [asked, setAsked] = useState<AskedRollback | null>(null);

  const rollBack = (r: Release) => {
    const to = r.rollback?.to;
    if (!to) return;
    setAsked({
      target: { act: "rollback", buildId: to.id },
      copy: LIFECYCLE_COPY.rollback(to.build_id, r.build?.build_id ?? null)
    });
  };

  return (
    <section className='flex flex-col gap-2' data-testid='admin-app-releases'>
      <div className='flex items-center justify-between gap-2'>
        <h3 className='font-medium text-xs'>Releases</h3>
        <fieldset className='m-0 flex gap-1 border-0 p-0' aria-label='Window'>
          {RELEASE_WINDOWS.map((n) => (
            <Button
              key={n}
              size='sm'
              variant={n === days ? "secondary" : "ghost"}
              className='h-6 px-2 text-xs'
              aria-pressed={n === days}
              onClick={() => setDays(n)}
              data-testid={`admin-app-releases-window-${n}`}
            >
              {n}d
            </Button>
          ))}
        </fieldset>
      </div>
      <AdminAsync
        query={releases}
        noun='this app&rsquo;s releases'
        rows={3}
        // A release serving since before the window has no event in it: still a release.
        isEmpty={(d) => d.releases.length === 0 && d.lanes.every((l) => l.events.length === 0)}
        empty={
          <p className='text-muted-foreground text-xs' data-testid='admin-app-releases-empty'>
            Nothing moved in the last {days} day{days === 1 ? "" : "s"}, and production serves
            nothing.
          </p>
        }
      >
        {(data) => <Timeline data={data} onOpenFunction={onOpenFunction} onRollback={rollBack} />}
      </AdminAsync>
      <PlannedMoveConfirm
        appId={app.id}
        target={asked?.target ?? null}
        copy={asked?.copy ?? null}
        onClose={() => setAsked(null)}
        testId='admin-app-releases-rollback-confirm'
      />
    </section>
  );
};

const Timeline = ({
  data,
  onOpenFunction,
  onRollback
}: {
  data: ReleaseTimeline;
  onOpenFunction: (name: string) => void;
  onRollback: (release: Release) => void;
}) => {
  const byIndex = (i: number | null) => data.releases.find((r) => r.index === i) ?? null;
  return (
    <>
      <ReleaseLanes data={data} />
      {/* A part the server could not read is said once, never shown as "none". */}
      {data.parts.migrations === "unavailable" && (
        <p
          className='text-muted-foreground text-xs'
          data-testid='admin-app-releases-migrations-unavailable'
        >
          Couldn't read the migrations applied in this window.
        </p>
      )}
      {data.parts.issues === "unavailable" && (
        <p
          className='text-muted-foreground text-xs'
          data-testid='admin-app-releases-issues-unavailable'
        >
          Couldn't read which failures are new.
        </p>
      )}
      {data.events_truncated && (
        <p className='text-muted-foreground text-xs' data-testid='admin-app-releases-truncated'>
          More moves happened in this window than are shown; the oldest are left out.
        </p>
      )}
      <ol className='flex flex-col gap-2'>
        {data.releases.map((r) => (
          <ReleaseRow
            key={r.index}
            release={r}
            previous={byIndex(r.previous)}
            parts={data.parts}
            onOpenFunction={onOpenFunction}
            onRollback={onRollback}
          />
        ))}
      </ol>
      {data.unreleased_migrations.length > 0 && (
        <p className='text-xs' data-testid='admin-app-releases-unreleased-migrations'>
          Applied to production with no release (a promote that failed after them, or a build since
          pruned):{" "}
          <span className='font-mono'>
            {data.unreleased_migrations.map((m) => `${m.store}/${m.filename}`).join(", ")}
          </span>
        </p>
      )}
    </>
  );
};
