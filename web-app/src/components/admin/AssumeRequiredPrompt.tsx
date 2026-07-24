import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState, useSyncExternalStore } from "react";
import { toast } from "sonner";
import { AssumeRoleDialog } from "@/components/admin/AssumeRoleDialog";
import {
  clearAssumeRequired,
  getAssumeRequiredSnapshot,
  subscribeAssumeRequired
} from "@/libs/utils/assumeRequired";

/**
 * Turns the staff assume-role refusal into an explanation plus the one action
 * that resolves it.
 *
 * Oxy staff are not members of a tenant org, so the workspace middleware refuses
 * them by design (#2710 closed the silent override). Before this, that surfaced
 * as "You don't have permission to do this." — which reads as a bug and sends
 * operators hunting through releases instead of assuming a role.
 *
 * A toast rather than an auto-opened modal: the refusal usually arrives from a
 * background query, and a page firing several of them must not seize the screen.
 * The toast carries the server's own wording (so backend and UI can't drift) and
 * the dialog stays the consent moment.
 */
export function AssumeRequiredPrompt() {
  const { org, message } = useSyncExternalStore(
    subscribeAssumeRequired,
    getAssumeRequiredSnapshot,
    getAssumeRequiredSnapshot
  );
  const [dialogOpen, setDialogOpen] = useState(false);
  const queryClient = useQueryClient();

  const orgId = org?.id ?? null;
  const orgName = org?.name ?? null;

  useEffect(() => {
    if (!orgId || !orgName) return;
    toast.error(`Viewing ${orgName} needs an assumed role`, {
      description:
        message ??
        "Oxy staff do not get ambient access to tenant data. Start a session for this org to continue — it is time-boxed and audited.",
      duration: 12_000,
      action: { label: "Assume role", onClick: () => setDialogOpen(true) },
      // Sonner fires onDismiss for swipe/close but onAutoClose for the timeout —
      // they are distinct. Clearing on only one leaves the signal set, and
      // `reportAssumeRequired` dedupes by org, so every later refusal for that
      // org would be swallowed silently.
      onDismiss: clearAssumeRequired,
      onAutoClose: clearAssumeRequired
    });
  }, [orgId, orgName, message]);

  if (!org) return null;

  return (
    <AssumeRoleDialog
      open={dialogOpen}
      onOpenChange={(next) => {
        setDialogOpen(next);
        // Closing without starting clears the signal, so a later refusal for
        // the same org can raise a fresh prompt instead of being deduped away.
        if (!next) clearAssumeRequired();
      }}
      org={org}
      onStarted={() => {
        setDialogOpen(false);
        clearAssumeRequired();
        // The identity behind every in-flight query just changed; refetch so the
        // page that just 403'd fills in rather than sitting on a stale error.
        queryClient.invalidateQueries();
      }}
    />
  );
}
