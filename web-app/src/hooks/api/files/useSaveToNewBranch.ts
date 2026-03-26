import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useSwitchProjectBranch } from "@/hooks/api/projects/useProjects";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import useIdeBranch from "@/stores/useIdeBranch";
import queryKeys from "../queryKey";

/**
 * Saves the current file content to a new auto-named branch (`edit/YYYY-MM-DD-HHmmss`).
 *
 * Used in force-edit-on-main mode: creates the worktree, saves the file, then
 * switches the IDE context to the new branch so the user continues on it
 * seamlessly.
 */
export function useSaveToNewBranch() {
  const { project, branchName: originalBranch } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const { setCurrentBranch } = useIdeBranch();
  const switchBranch = useSwitchProjectBranch();

  const saveToNewBranch = async (
    pathb64: string,
    content: string,
    onSuccess?: () => void
  ): Promise<void> => {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const timestamp = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
    const newBranch = `edit/${timestamp}`;

    // 1. Create the git worktree for the new branch
    await switchBranch.mutateAsync({ projectId: project.id, branchName: newBranch });

    // 2. Save file — if this fails, roll back the IDE to the original branch
    try {
      await FileService.saveFile(project.id, pathb64, content, newBranch);
    } catch (err) {
      setCurrentBranch(project.id, originalBranch);
      throw err;
    }

    // 3. Invalidate the file cache for the new branch so it reloads cleanly
    queryClient.removeQueries({
      queryKey: queryKeys.file.get(project.id, newBranch, pathb64)
    });

    // 4. Switch the IDE to the new branch — triggers a re-render everywhere
    setCurrentBranch(project.id, newBranch);

    toast.success(`Saved to "${newBranch}"`);
    onSuccess?.();
  };

  return { saveToNewBranch, isPending: switchBranch.isPending };
}
