import { GitBranch, Globe, HardDrive, Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { GitNamespaceSelection } from "@/components/GitNamespaceSelection";
import GithubIcon from "@/components/ui/GithubIcon";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Separator } from "@/components/ui/shadcn/separator";
import { useAuth } from "@/contexts/AuthContext";
import { useGitHubBranchesWithApp, useGitHubRepositoriesWithApp } from "@/hooks/api/github";
import useAddRepositoryFromGitHub from "@/hooks/api/repositories/useAddRepositoryFromGitHub";
import useRemoveRepository from "@/hooks/api/repositories/useRemoveRepository";
import useRepositories from "@/hooks/api/repositories/useRepositories";
import PageHeader from "@/pages/ide/components/PageHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { Repository } from "@/types/repository";

function AddRepositoryDialog({
  open,
  onOpenChange
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
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
    refetch: refetchRepos,
    isFetching: isFetchingRepos
  } = useGitHubRepositoriesWithApp(orgId, namespaceId);

  const { data: branches = [], isPending: isLoadingBranches } = useGitHubBranchesWithApp(
    orgId,
    namespaceId,
    repoName
  );

  const reset = () => {
    setNamespaceId("");
    setRepoId(null);
    setRepoName("");
    setCloneUrl("");
    setBranch("");
  };

  const handleClose = () => {
    reset();
    onOpenChange(false);
  };

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
    // Use repo short name (last segment of full_name) as the display name
    const name = repoName.split("/").pop() ?? repoName;
    await addRepo.mutateAsync({ name, git_namespace_id: namespaceId, clone_url: cloneUrl, branch });
    handleClose();
  };

  const canSubmit = !!namespaceId && repoId !== null && !!branch && !addRepo.isPending;

  const repoItems = repositories.map((r) => ({
    value: String(r.id),
    label: r.full_name,
    searchText: r.full_name
  }));

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className='max-w-lg p-0'>
        <DialogHeader className='p-6 pb-0'>
          <DialogTitle>Link a repository</DialogTitle>
          <DialogDescription>
            Connect a GitHub repository to browse and edit its files in the IDE.
          </DialogDescription>
        </DialogHeader>

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
              <Select value={branch} onValueChange={setBranch} disabled={isLoadingBranches}>
                <SelectTrigger>
                  <SelectValue placeholder={isLoadingBranches ? "Loading…" : "Select branch"} />
                </SelectTrigger>
                <SelectContent>
                  {isLoadingBranches ? (
                    <SelectItem value='loading' disabled>
                      <div className='flex items-center gap-2'>
                        <Loader2 className='h-4 w-4 animate-spin' />
                        Loading…
                      </div>
                    </SelectItem>
                  ) : (
                    branches.map((b) => (
                      <SelectItem key={b.name} value={b.name}>
                        {b.name}
                      </SelectItem>
                    ))
                  )}
                </SelectContent>
              </Select>
            </div>
          )}

          <div className='flex justify-end gap-2 pt-1'>
            <Button type='button' variant='outline' onClick={handleClose} size='sm'>
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
                  <Plus className='h-3.5 w-3.5' />
                  Link repository
                </>
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RepositoryRow({ repo }: { repo: Repository }) {
  const removeRepo = useRemoveRepository();
  const [confirmOpen, setConfirmOpen] = useState(false);

  return (
    <>
      <div className='flex items-center gap-3 rounded-md border border-border/40 px-4 py-3 transition-colors hover:border-border/70 hover:bg-accent/20'>
        <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary/10'>
          {repo.git_namespace_id ? (
            <GithubIcon className='h-4 w-4 text-primary' />
          ) : repo.git_url ? (
            <Globe className='h-4 w-4 text-primary' />
          ) : (
            <HardDrive className='h-4 w-4 text-primary' />
          )}
        </div>

        <div className='min-w-0 flex-1'>
          <div className='flex items-center gap-2'>
            <span className='font-medium font-mono text-sm'>{repo.name}</span>
            <code className='rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground'>
              @{repo.name}/
            </code>
          </div>
          <p className='mt-0.5 truncate font-mono text-[11px] text-muted-foreground/70'>
            {repo.git_url ?? repo.path}
          </p>
        </div>

        <div className='flex shrink-0 items-center gap-2'>
          {repo.git_url ? (
            <Badge variant='secondary' className='gap-1 text-[10px]'>
              <GitBranch className='h-2.5 w-2.5' />
              {repo.branch ?? "HEAD"}
            </Badge>
          ) : (
            <Badge variant='outline' className='text-[10px]'>
              local
            </Badge>
          )}
          <CanWorkspaceAdmin>
            <Button
              variant='ghost'
              size='sm'
              onClick={() => setConfirmOpen(true)}
              disabled={removeRepo.isPending}
              className='h-7 w-7 p-0 text-muted-foreground hover:text-destructive'
              tooltip='Remove repository'
            >
              {removeRepo.isPending ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' />
              ) : (
                <Trash2 className='h-3.5 w-3.5' />
              )}
            </Button>
          </CanWorkspaceAdmin>
        </div>
      </div>

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className='max-w-sm'>
          <DialogHeader>
            <DialogTitle>Remove repository?</DialogTitle>
            <DialogDescription>
              <strong>{repo.name}</strong> will be removed from this project's config. The source
              files are not deleted.
            </DialogDescription>
          </DialogHeader>
          <div className='flex justify-end gap-2 pt-2'>
            <Button variant='outline' size='sm' onClick={() => setConfirmOpen(false)}>
              Cancel
            </Button>
            <Button
              variant='destructive'
              size='sm'
              onClick={() => {
                removeRepo.mutate(repo.name);
                setConfirmOpen(false);
              }}
            >
              Remove
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

export default function RepositoriesPage() {
  const { isLocalMode } = useAuth();
  const { data: repos = [], isLoading } = useRepositories();
  const [addOpen, setAddOpen] = useState(false);

  if (isLocalMode) return null;

  return (
    <div className='flex h-full flex-col'>
      <PageHeader
        icon={GitBranch}
        title='Repositories'
        actions={
          <CanWorkspaceAdmin>
            <Button size='sm' variant='outline' onClick={() => setAddOpen(true)}>
              <Plus className='h-3.5 w-3.5' />
              Add repository
            </Button>
          </CanWorkspaceAdmin>
        }
      />

      <div className='customScrollbar min-h-0 flex-1 overflow-auto p-6'>
        <div className='mb-4'>
          <p className='text-muted-foreground text-sm'>
            Link external repositories to surface their files alongside your Oxy project in the IDE.
            Suitable for dbt projects, LookML repos, SQL model libraries, or any other data modeling
            repo.
          </p>
        </div>

        <Separator className='mb-6' />

        {isLoading ? (
          <div className='flex items-center justify-center py-12'>
            <Loader2 className='h-5 w-5 animate-spin text-muted-foreground' />
          </div>
        ) : repos.length === 0 ? (
          <div className='flex flex-col items-center justify-center gap-3 rounded-lg border border-border/50 border-dashed py-12 text-center'>
            <GitBranch className='h-8 w-8 text-muted-foreground/40' />
            <div>
              <p className='font-medium text-muted-foreground text-sm'>No repositories linked</p>
              <p className='mt-1 text-[12px] text-muted-foreground/60'>
                Add a dbt, LookML, or other data modeling repo to browse its files in the IDE.
              </p>
            </div>
            <CanWorkspaceAdmin>
              <Button size='sm' variant='outline' onClick={() => setAddOpen(true)}>
                <Plus className='h-3.5 w-3.5' />
                Add repository
              </Button>
            </CanWorkspaceAdmin>
          </div>
        ) : (
          <div className='flex flex-col gap-2'>
            {repos.map((repo) => (
              <RepositoryRow key={repo.name} repo={repo} />
            ))}
          </div>
        )}
      </div>

      <AddRepositoryDialog open={addOpen} onOpenChange={setAddOpen} />
    </div>
  );
}
