import { toast } from "sonner";
import { useRollbackApp } from "@/hooks/api/customApps/useAppBuilds";
import { usePublishApp, useUnpublishApp } from "@/hooks/api/customApps/useCustomApps";
import { errMessage } from "@/hooks/api/errMessage";
import { useSetAppPublished } from "@/hooks/api/partners";
import type { AppBuild } from "@/types/apps";
import { shortBuildId } from "./lifecycleCopy";

/**
 * What a surface can *do* to an app's lifecycle. The card renders a control for each
 * verb present and nothing for one absent, so the partner console — which can publish
 * and unpublish but has no build history to roll back through — hands the same card a
 * narrower set rather than a different card.
 *
 * Every verb resolves when the server has answered, so the confirm can close on
 * success and stay open (with the error toasted) on failure.
 */
export interface LifecycleActions {
  /** Point the live channel at the draft build (`POST /customer-apps/{id}/publish`). */
  promoteDraft?: () => Promise<void>;
  /** First publish; same endpoint, read differently by an app with nothing live. */
  publish?: () => Promise<void>;
  /** Move the live pointer to a retained build. */
  rollbackTo?: (build: AppBuild) => Promise<void>;
  unpublish?: () => Promise<void>;
  /** Any verb in flight — every control disables together. */
  pending: boolean;
}

/** The staff console's verbs — the admin `/customer-apps/{id}/*` mutations. */
export function useAdminLifecycleActions(appId: string): LifecycleActions {
  const publish = usePublishApp();
  const unpublish = useUnpublishApp();
  const rollback = useRollbackApp();

  const promote = async () => {
    await publish.mutateAsync(appId);
  };

  return {
    promoteDraft: promote,
    publish: promote,
    rollbackTo: async (build) => {
      try {
        await rollback.mutateAsync({ id: appId, buildId: build.id });
        toast.success(`Build ${shortBuildId(build.build_id)} is now live.`);
      } catch (err) {
        toast.error(errMessage(err, "Couldn't change the live build"));
        throw err;
      }
    },
    unpublish: async () => {
      await unpublish.mutateAsync(appId);
    },
    pending: publish.isPending || unpublish.isPending || rollback.isPending
  };
}

/**
 * The partner console's verbs —`/partners/{id}/apps/{appId}/publish`, gated by the
 * `manage_apps` ceiling server-side. No build history on this surface, so no rollback.
 */
export function usePartnerLifecycleActions(
  partnerId: string,
  orgId: string,
  appId: string
): LifecycleActions {
  const set = useSetAppPublished(partnerId);
  const run = async (published: boolean) => {
    try {
      await set.mutateAsync({ appId, orgId, published });
      toast.success(published ? "Published" : "Unpublished");
    } catch (err) {
      toast.error(errMessage(err, published ? "Couldn't publish" : "Couldn't unpublish"));
      throw err;
    }
  };
  return {
    publish: () => run(true),
    unpublish: () => run(false),
    pending: set.isPending
  };
}
