import { Key, Loader2, Plus, X } from "lucide-react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import type { GitHubNamespace } from "@/types/github";

interface Props {
  namespaces: GitHubNamespace[];
  isLoading: boolean;
  value?: string;
  onChange?: (value: string) => void;
  onDelete: (ns: GitHubNamespace) => void;
  onAdd: () => void;
}

export default function NamespaceList({
  namespaces,
  isLoading,
  value,
  onChange,
  onDelete,
  onAdd
}: Props) {
  if (isLoading) {
    return (
      <div className='flex items-center gap-2 text-muted-foreground text-sm'>
        <Loader2 className='h-4 w-4 animate-spin' />
        Loading accounts…
      </div>
    );
  }

  return (
    <div className='space-y-2'>
      {namespaces.length > 0 && (
        <div className='flex flex-col gap-1.5'>
          {namespaces.map((ns) => {
            const isPAT = ns.slug === "pat";
            const isSelected = ns.id === value;
            return (
              <div
                key={ns.id}
                className={`flex items-center gap-3 rounded-lg border px-3 py-2.5 transition-colors ${
                  isSelected ? "border-primary bg-primary/5" : "border-border bg-card"
                }`}
              >
                <button
                  type='button'
                  onClick={() => onChange?.(ns.id)}
                  className='flex flex-1 items-center gap-3 text-left'
                >
                  <GithubIcon className='h-4 w-4 shrink-0 text-muted-foreground' />
                  <span className='flex-1 truncate font-medium text-sm'>{ns.name}</span>
                  <Badge variant='outline' className='shrink-0 gap-1 text-xs'>
                    {isPAT ? (
                      <>
                        <Key className='h-3 w-3' />
                        PAT
                      </>
                    ) : (
                      <>
                        <GithubIcon className='h-3 w-3' />
                        App
                      </>
                    )}
                  </Badge>
                  {isSelected && <div className='h-1.5 w-1.5 shrink-0 rounded-full bg-primary' />}
                </button>
                <button
                  type='button'
                  onClick={() => onDelete(ns)}
                  className='shrink-0 text-muted-foreground/50 transition-colors hover:text-destructive'
                  aria-label={`Remove ${ns.name}`}
                >
                  <X className='h-3.5 w-3.5' />
                </button>
              </div>
            );
          })}
        </div>
      )}

      <Button variant='outline' size='sm' className='w-full gap-2' onClick={onAdd}>
        <Plus className='h-3.5 w-3.5' />
        {namespaces.length === 0 ? "Connect a GitHub account" : "Connect another account"}
      </Button>
    </div>
  );
}
