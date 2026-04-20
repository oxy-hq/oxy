import { AlertTriangle, FolderOpen, Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useGitHubBranchesWithApp, useGitHubRepositoriesWithApp } from "@/hooks/api/github";
import useAddRepositoryFromGitHub from "@/hooks/api/repositories/useAddRepositoryFromGitHub";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { GitNamespaceSelection } from "./GitNamespaceSelection";
import { Button } from "./ui/shadcn/button";
import { Combobox } from "./ui/shadcn/combobox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "./ui/shadcn/dialog";
import { Label } from "./ui/shadcn/label";

// ─── GitHub mode: namespace + repo + branch picker ───────────────────────────

function GitHubLinkForm({ onClose }: { onClose: () => void }) {
  const { org } = useCurrentOrg();
  const orgId = org?.id ?? "";
  const addRepo = useAddRepositoryFromGitHub();
  const [namespaceId, setNamespaceId] = useState("");
  const [repoId, setRepoId] = useState<number | null>(null);
  const [repoName, setRepoName] = useState("");
  const [cloneUrl, setCloneUrl] = useState("");
  const [branch, setBranch] = useState("");

  const {
    data: repositories = [],
    isPending: isLoadingRepos,
    error: repoError,
    refetch: refetchRepos,
    isFetching: isFetchingRepos
  } = useGitHubRepositoriesWithApp(orgId, namespaceId);

  const { data: branches = [], isPending: isLoadingBranches } = useGitHubBranchesWithApp(
    orgId,
    namespaceId,
    repoName
  );

  const repoErrorStatus = (repoError as { response?: { status?: number } } | null)?.response
    ?.status;
  const isRepoAuthError = repoErrorStatus === 403 || repoErrorStatus === 404;

  const handleRepoChange = (value: string) => {
    const repo = repositories.find((r) => String(r.id) === value);
    if (repo) {
      setRepoId(repo.id);
      setRepoName(repo.full_name);
      setCloneUrl(repo.clone_url);
      setBranch(repo.default_branch);
    }
  };

  const handleSubmit = async () => {
    if (!namespaceId || repoId === null || !branch || !cloneUrl) return;
    const name = repoName.split("/").pop() ?? repoName;
    await addRepo.mutateAsync({ name, git_namespace_id: namespaceId, clone_url: cloneUrl, branch });
    onClose();
  };

  const canSubmit = !!namespaceId && repoId !== null && !!branch && !addRepo.isPending;

  const repoItems = repositories.map((r) => ({
    value: String(r.id),
    label: r.full_name,
    searchText: r.full_name
  }));

  return (
    <div className='flex flex-col gap-5 p-6 pt-4'>
      <GitNamespaceSelection value={namespaceId} onChange={setNamespaceId} />

      {namespaceId && (
        <div className='space-y-2'>
          <div className='flex items-center justify-between'>
            <Label>Repository</Label>
            <button
              type='button'
              onClick={() => refetchRepos()}
              disabled={isFetchingRepos}
              className='flex items-center gap-1 text-muted-foreground/50 text-xs transition-colors hover:text-muted-foreground disabled:opacity-40'
            >
              <RefreshCw className={`h-3 w-3 ${isFetchingRepos ? "animate-spin" : ""}`} />
              Refresh
            </button>
          </div>
          {isLoadingRepos ? (
            <div className='flex h-9 items-center gap-2 rounded-md border border-input bg-input/30 px-3 text-muted-foreground text-sm'>
              <Loader2 className='h-3.5 w-3.5 animate-spin' />
              Loading repositories…
            </div>
          ) : isRepoAuthError ? (
            <div className='flex items-start gap-2.5 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2.5 dark:border-amber-800/40 dark:bg-amber-950/20'>
              <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-600 dark:text-amber-400' />
              <div className='min-w-0 flex-1'>
                <p className='text-amber-700 text-xs leading-snug dark:text-amber-400'>
                  {repoErrorStatus === 404
                    ? "GitHub App installation not found."
                    : "Could not access GitHub. Token or app permissions may have expired."}
                </p>
                <button
                  type='button'
                  onClick={() => setNamespaceId("")}
                  className='mt-1 text-amber-700 text-xs underline underline-offset-2 hover:text-amber-900 dark:text-amber-400 dark:hover:text-amber-200'
                >
                  Reconnect account
                </button>
              </div>
            </div>
          ) : (
            <Combobox
              items={repoItems}
              value={repoId !== null ? String(repoId) : ""}
              onValueChange={handleRepoChange}
              placeholder='Select repository'
              searchPlaceholder='Search repositories…'
            />
          )}
        </div>
      )}

      {namespaceId && repoId !== null && (
        <div className='space-y-2'>
          <Label>Branch</Label>
          {isLoadingBranches ? (
            <div className='flex h-9 items-center gap-2 rounded-md border border-input bg-input/30 px-3 text-muted-foreground text-sm'>
              <Loader2 className='h-3.5 w-3.5 animate-spin' />
              Loading branches…
            </div>
          ) : (
            <Combobox
              items={branches.map((b) => ({ value: b.name, label: b.name, searchText: b.name }))}
              value={branch}
              onValueChange={setBranch}
              placeholder='Select branch'
              searchPlaceholder='Search branches…'
            />
          )}
        </div>
      )}

      {repoId !== null && (
        <div className='rounded-md border border-border/50 bg-muted/30 px-3 py-2.5 text-muted-foreground text-xs leading-relaxed'>
          Cloned to{" "}
          <code className='rounded bg-muted px-1 font-mono text-[11px]'>
            .repositories/{repoName.split("/").pop() || "repo"}/
          </code>{" "}
          relative to your project and added to{" "}
          <code className='rounded bg-muted px-1 font-mono text-[11px]'>.gitignore</code>.
        </div>
      )}

      <div className='flex justify-end gap-2 pt-1'>
        <Button type='button' variant='outline' onClick={onClose} size='sm'>
          Cancel
        </Button>
        <Button size='sm' disabled={!canSubmit} onClick={handleSubmit}>
          {addRepo.isPending ? (
            <>
              <Loader2 className='h-3.5 w-3.5 animate-spin' />
              Linking…
            </>
          ) : (
            <>
              <FolderOpen className='h-3.5 w-3.5' />
              Link repository
            </>
          )}
        </Button>
      </div>
    </div>
  );
}

// ─── Main dialog ──────────────────────────────────────────────────────────────

interface LinkRepoDialogProps {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}

export function LinkRepoDialog({ open, onOpenChange }: LinkRepoDialogProps) {
  // Reset internal form state when the dialog closes.
  const [key, setKey] = useState(0);
  useEffect(() => {
    if (!open) setKey((k) => k + 1);
  }, [open]);

  const handleClose = () => onOpenChange(false);

  return (
    <Dialog open={open} onOpenChange={(v) => !v && handleClose()}>
      <DialogContent className='max-w-lg p-0'>
        <DialogHeader className='p-6 pb-0'>
          <DialogTitle>Link a repository</DialogTitle>
          <DialogDescription>
            Connect a GitHub repository to browse and edit its files in the IDE.
          </DialogDescription>
        </DialogHeader>
        <GitHubLinkForm key={key} onClose={handleClose} />
      </DialogContent>
    </Dialog>
  );
}
