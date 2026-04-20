import { Check, ChevronDown, GitFork, Link2, Loader2, Trash2 } from "lucide-react";
import { useState } from "react";
import { LinkRepoDialog } from "@/components/LinkRepoDialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import useFileTree from "@/hooks/api/files/useFileTree";
import useRemoveRepository from "@/hooks/api/repositories/useRemoveRepository";
import useSelectedRepo from "@/stores/useSelectedRepo";
import useTheme from "@/stores/useTheme";

export function RepoSwitcher({ isReadOnly }: { isReadOnly: boolean }) {
  const { data: fileTree } = useFileTree();
  const { selectedRepo, setSelectedRepo } = useSelectedRepo();
  const { theme } = useTheme();
  const [linkOpen, setLinkOpen] = useState(false);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  const remove = useRemoveRepository();

  const repos = fileTree?.repositories ?? [];
  const isPrimary = selectedRepo === "primary";
  const selectedRepoData = isPrimary ? undefined : repos.find((r) => r.name === selectedRepo);
  const isCloning = selectedRepoData?.sync_status === "cloning";
  const label = isPrimary ? "Root" : selectedRepo;
  const oxyLogo = theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg";

  const handleRemove = (name: string) => {
    remove.mutate(name, {
      onSuccess: () => {
        if (selectedRepo === name) setSelectedRepo("primary");
        setPendingRemove(null);
      },
      onError: () => setPendingRemove(null)
    });
  };

  if (repos.length === 0 && isReadOnly) return null;

  return (
    <>
      <DropdownMenu
        onOpenChange={(open) => {
          if (!open) setPendingRemove(null);
        }}
      >
        <DropdownMenuTrigger asChild>
          <button
            type='button'
            className='flex h-7 items-center gap-1.5 rounded border border-border/50 bg-transparent px-2 text-xs transition-colors hover:border-border hover:bg-accent/40'
          >
            {isCloning ? (
              <Loader2 className='h-3 w-3 shrink-0 animate-spin text-muted-foreground/60' />
            ) : isPrimary ? (
              <img src={oxyLogo} alt='Oxy' className='h-3 w-3 shrink-0' />
            ) : (
              <GitFork className='h-3 w-3 shrink-0 text-muted-foreground/60' />
            )}
            <span className='max-w-28 truncate text-muted-foreground'>{label}</span>
            {isCloning && (
              <span className='truncate text-[10px] text-muted-foreground/50'>cloning…</span>
            )}
            <ChevronDown className='h-3 w-3 shrink-0 text-muted-foreground/40' />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-52'>
          <DropdownMenuItem
            onClick={() => setSelectedRepo("primary")}
            className='flex items-center gap-2'
          >
            <img src={oxyLogo} alt='Oxy' className='h-3.5 w-3.5 shrink-0' />
            <span className='flex-1'>Root</span>
            {isPrimary && <Check className='h-3 w-3 text-primary' />}
          </DropdownMenuItem>
          {repos.map((repo) =>
            pendingRemove === repo.name ? (
              <div key={repo.name} className='flex items-center gap-1 px-2 py-1.5'>
                <span className='flex-1 truncate text-destructive text-xs'>
                  Remove {repo.name}?
                </span>
                <button
                  type='button'
                  onClick={() => handleRemove(repo.name)}
                  disabled={remove.isPending}
                  className='rounded px-1.5 py-0.5 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50'
                >
                  {remove.isPending ? <Loader2 className='h-3 w-3 animate-spin' /> : "Yes"}
                </button>
                <button
                  type='button'
                  onClick={() => setPendingRemove(null)}
                  className='rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent'
                >
                  No
                </button>
              </div>
            ) : (
              <DropdownMenuItem
                key={repo.name}
                onClick={() => setSelectedRepo(repo.name)}
                className='group flex items-center gap-2'
              >
                <GitFork className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
                <span className='flex-1 truncate'>{repo.name}</span>
                {repo.sync_status === "cloning" && (
                  <Loader2 className='h-3 w-3 animate-spin text-muted-foreground/50' />
                )}
                {selectedRepo === repo.name && repo.sync_status !== "cloning" && (
                  <Check className='h-3 w-3 text-primary' />
                )}
                {!isReadOnly && (
                  <button
                    type='button'
                    onClick={(e) => {
                      e.stopPropagation();
                      setPendingRemove(repo.name);
                    }}
                    className='ml-auto hidden rounded p-0.5 text-muted-foreground/40 hover:text-destructive group-hover:flex'
                  >
                    <Trash2 className='h-3 w-3' />
                  </button>
                )}
              </DropdownMenuItem>
            )
          )}
          {!isReadOnly && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => setLinkOpen(true)}
                className='flex items-center gap-2'
              >
                <Link2 className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
                <span className='flex-1'>Link repository…</span>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <LinkRepoDialog open={linkOpen} onOpenChange={setLinkOpen} />
    </>
  );
}
