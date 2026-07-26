import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogDestructiveAction,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from "@/components/ui/shadcn/alert-dialog";
import { useDeleteWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import { releaseBodyPointerLock } from "@/libs/utils/pointerEvents";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

/**
 * "Cancel setup" for the workspace onboarding wizard.
 *
 * Unlike "Start over" (which resets config but keeps the workspace, so the home
 * guard bounces the user right back here), this deletes the in-progress
 * workspace outright and returns to the org — which routes to another workspace,
 * or the clean "create your first workspace" screen if this was the only one.
 *
 * Only org owners/admins can delete a workspace (the server requires it), so the
 * control hides for anyone else — they still have the org switcher to leave.
 */
export default function CancelSetupButton({
  workspaceId,
  onBeforeDelete
}: {
  workspaceId: string;
  /** Stop any in-flight build first so its runs don't write to a deleted workspace. */
  onBeforeDelete?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const orgId = useCurrentOrg((s) => s.org?.id) ?? "";
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const role = useCurrentOrg((s) => s.role);
  const del = useDeleteWorkspace();

  const canDelete = role === "owner" || role === "admin";
  if (!canDelete || !orgId || !workspaceId) return null;

  const confirm = () => {
    onBeforeDelete?.();
    del.mutate(
      { orgId, id: workspaceId, deleteFiles: true },
      {
        onSuccess: () => {
          setOpen(false);
          // Defer the navigate past the dialog's close so no body pointer-events
          // lock leaks onto the destination page.
          requestAnimationFrame(() => {
            releaseBodyPointerLock();
            navigate(orgSlug ? ROUTES.ORG(orgSlug).ROOT : ROUTES.ROOT, { replace: true });
          });
        },
        onError: () => toast.error("Couldn't cancel setup. Try again.")
      }
    );
  };

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger asChild>
        <button
          type='button'
          data-testid='onboarding-cancel-setup'
          className='text-destructive text-xs hover:underline'
        >
          Cancel setup
        </button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Cancel setup and delete this workspace?</AlertDialogTitle>
          <AlertDialogDescription>
            This permanently deletes the workspace and everything set up so far. You can create a
            new one afterwards. This can&apos;t be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Keep setup</AlertDialogCancel>
          <AlertDialogDestructiveAction onClick={confirm} disabled={del.isPending}>
            {del.isPending ? "Deleting…" : "Delete workspace"}
          </AlertDialogDestructiveAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
