import { useState } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import type { AppLifecycle as Lifecycle } from "@/hooks/api/customApps/deriveLifecycle";
import { useAppLifecycle } from "@/hooks/api/customApps/useAppLifecycle";
import { useMissingSecretsForBuild } from "@/hooks/api/customApps/useAppSecrets";
import type { AppAccessSummary } from "@/types/appAccess";
import type { CustomApp } from "@/types/apps";
import { lifecycleVerdict } from "../../appStatus";
import { MissingSecretsWarning } from "../AppDetail/components/MissingSecretsWarning";
import { useAdminLifecycleActions, usePartnerLifecycleActions } from "./actions";
import { LifecycleCard, type LifecycleTarget, type LifecycleVariant } from "./LifecycleCard";

export { LifecycleCard } from "./LifecycleCard";
export { LifecycleConfirm } from "./LifecycleConfirm";
export { LifecycleVerdictPill } from "./LifecycleVerdictPill";
export { LIFECYCLE_COPY } from "./lifecycleCopy";

/**
 * The card on a staff surface: the console, the dossier, the settings panel.
 * Reads through `useAppLifecycle` (shared cache with every other panel) and acts
 * through the admin `/customer-apps/{id}/*` mutations.
 *
 * Its confirm also names the `required` secrets the build about to go live declares
 * and production has no value for (`GET …/secrets?build=`), and holds the verb until
 * that check has answered. A warning here, not a refusal: the console's promote and
 * rollback do not block on it — only `oxyc promote` does.
 */
export const AppLifecycle = ({
  app,
  variant = "full",
  className
}: {
  app: CustomApp;
  variant?: LifecycleVariant;
  className?: string;
}) => {
  const lifecycle = useAppLifecycle(app);
  const actions = useAdminLifecycleActions(app.id);
  const [target, setTarget] = useState<LifecycleTarget | null>(null);
  // The warning's own query (same key, so one request): the confirm waits for it.
  const secretsCheck = useMissingSecretsForBuild(app.id, target?.buildId);

  if (lifecycle.isLoading) {
    return <Skeleton className='h-24 w-full rounded-lg' data-testid='app-lifecycle-loading' />;
  }
  if (lifecycle.error) {
    return (
      <p className='text-destructive text-xs' data-testid='app-lifecycle-error'>
        Couldn't load this app's builds.
      </p>
    );
  }
  return (
    <LifecycleCard
      lifecycle={lifecycle}
      actions={actions}
      orgSlug={app.org_slug}
      variant={variant}
      className={className}
      onTargetChange={setTarget}
      preflight={
        target
          ? {
              notice: (
                <MissingSecretsWarning appId={app.id} buildId={target.buildId} act={target.act} />
              ),
              hold: secretsCheck.isLoading
            }
          : undefined
      }
    />
  );
};

/**
 * The same card on the partner console, where the backend offers publish /
 * unpublish and no build history. The lifecycle is therefore the publish fact alone:
 * no live build, no draft, and a verdict from `published` — the partner API has no
 * fleet-health or availability read, so nothing here claims one.
 */
export const PartnerAppLifecycle = ({
  app,
  partnerId,
  orgId,
  orgSlug,
  variant = "row",
  className
}: {
  app: Pick<AppAccessSummary, "id" | "published">;
  partnerId: string;
  orgId: string;
  orgSlug?: string | null;
  variant?: LifecycleVariant;
  className?: string;
}) => {
  const actions = usePartnerLifecycleActions(partnerId, orgId, app.id);
  return (
    <LifecycleCard
      lifecycle={partnerLifecycle(app.published)}
      actions={actions}
      orgSlug={orgSlug ?? null}
      variant={variant}
      className={className}
    />
  );
};

/** A lifecycle built from the one fact the partner API returns. */
export function partnerLifecycle(published: boolean): Lifecycle {
  return {
    liveBuild: null,
    draftBuild: null,
    draftAhead: false,
    published,
    publishedAt: null,
    verdict: lifecycleVerdict({ published }),
    heldCount: null,
    heldTruncated: false,
    lastPromoted: { at: null, byEmail: null },
    builds: [],
    isLoading: false,
    error: null
  };
}
