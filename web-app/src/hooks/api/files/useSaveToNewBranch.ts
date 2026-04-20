import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import { useSwitchWorkspaceBranch } from "@/hooks/api/workspaces/useWorkspaces";
import useCurrentWorkspaceBranch from "@/hooks/useCurrentWorkspaceBranch";
import { FileService, WorkspaceService } from "@/services/api";
import useIdeBranch from "@/stores/useIdeBranch";
import queryKeys from "../queryKey";

/** Derives a git-branch-safe slug from an email or display name. */
function toUserSlug(email?: string, name?: string): string {
  const raw = email ? email.split("@")[0] : (name ?? "");
  const slug = raw
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "user";
}

/**
 * Saves the current file content to a new auto-named branch
 * (`<user-slug>/YYYY-MM-DD-HHmmss`).
 *
 * Used in force-edit-on-main mode: creates the worktree, saves the file, then
 * switches the IDE context to the new branch so the user continues on it
 * seamlessly. The user slug makes branches unique across collaborators.
 */
export function useSaveToNewBranch() {
  const { workspace, branchName: originalBranch } = useCurrentWorkspaceBranch();
  const queryClient = useQueryClient();
  const { setCurrentBranch } = useIdeBranch();
  const switchBranch = useSwitchWorkspaceBranch();
  const { data: currentUser } = useCurrentUser();
  const [isSaving, setIsSaving] = useState(false);

  const saveToNewBranch = async (
    pathb64: string,
    content: string,
    onSuccess?: () => void
  ): Promise<void> => {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const timestamp = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
    const userSlug = toUserSlug(currentUser?.email, currentUser?.name);
    const newBranch = `${userSlug}/${timestamp}`;

    // 1. Create the git worktree for the new branch.  When `base_branch` is
    //    configured in config.yml, fork from it instead of whatever the server
    //    currently has checked out — this lets Oxy serve from a deployment
    //    branch while still forking new work from an integration branch.
    setIsSaving(true);
    try {
      await switchBranch.mutateAsync({
        workspaceId: workspace.id,
        branchName: newBranch
      });

      // 2. Save file — if this fails, clean up the orphaned worktree and roll back
      await FileService.saveFile(workspace.id, pathb64, content, newBranch);
    } catch (err) {
      setCurrentBranch(workspace.id, originalBranch);
      try {
        await WorkspaceService.deleteBranch(workspace.id, newBranch);
      } catch (cleanupErr) {
        console.error("Failed to clean up orphaned branch:", cleanupErr);
      }
      throw err;
    } finally {
      setIsSaving(false);
    }

    // 3. Invalidate the file cache for the new branch so it reloads cleanly
    queryClient.removeQueries({
      queryKey: queryKeys.file.get(workspace.id, newBranch, pathb64)
    });

    // 4. Switch the IDE to the new branch — triggers a re-render everywhere
    setCurrentBranch(workspace.id, newBranch);

    toast.success(`Saved to "${newBranch}"`);
    onSuccess?.();
  };

  return { saveToNewBranch, isPending: isSaving };
}
