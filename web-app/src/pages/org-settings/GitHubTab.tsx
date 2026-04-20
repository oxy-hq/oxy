import { Trash2 } from "lucide-react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { useDeleteGitNamespace } from "@/hooks/api/github/useDeleteGitNamespace";
import { useGitHubNamespaces } from "@/hooks/api/github/useGitHubNamespaces";
import type { Organization } from "@/types/organization";

const GithubIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox='0 0 24 24' fill='currentColor' aria-hidden='true'>
    <path d='M12 2C6.477 2 2 6.477 2 12c0 4.418 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.009-.868-.013-1.703-2.782.603-3.369-1.342-3.369-1.342-.454-1.154-1.11-1.462-1.11-1.462-.908-.62.069-.608.069-.608 1.003.07 1.532 1.031 1.532 1.031.891 1.528 2.341 1.087 2.91.831.091-.645.349-1.087.635-1.337-2.22-.252-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.03-2.682-.103-.253-.447-1.27.097-2.646 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0112 6.836a9.59 9.59 0 012.504.337c1.909-1.294 2.748-1.025 2.748-1.025.546 1.376.202 2.394.1 2.646.64.699 1.026 1.591 1.026 2.682 0 3.841-2.337 4.687-4.565 4.935.359.309.678.919.678 1.852 0 1.337-.012 2.414-.012 2.742 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z' />
  </svg>
);

interface GitHubTabProps {
  org: Organization;
}

export default function GitHubTab({ org }: GitHubTabProps) {
  const { data: namespaces, isLoading } = useGitHubNamespaces(org.id);
  const deleteNamespace = useDeleteGitNamespace();

  const canManage = org.role === "owner" || org.role === "admin";

  const handleDelete = async (id: string, name: string) => {
    try {
      await deleteNamespace.mutateAsync({ orgId: org.id, id });
      toast.success(`${name} disconnected`);
    } catch {
      toast.error("Failed to disconnect GitHub connection");
    }
  };

  if (isLoading) {
    return <div className='py-8 text-center text-muted-foreground text-sm'>Loading...</div>;
  }

  return (
    <div className='space-y-6'>
      <div className='space-y-1'>
        <h3 className='font-medium'>GitHub Connections</h3>
        <p className='text-muted-foreground text-sm'>
          GitHub App installations connected to this organization.
        </p>
      </div>

      {namespaces && namespaces.length > 0 ? (
        <div className='divide-y divide-border rounded-lg border border-border'>
          {namespaces.map((ns) => (
            <div key={ns.id} className='flex items-center gap-3 px-4 py-3'>
              <div className='flex h-8 w-8 items-center justify-center rounded-full bg-muted'>
                <GithubIcon className='h-4 w-4 text-muted-foreground' />
              </div>
              <div className='min-w-0 flex-1'>
                <div className='truncate font-medium text-sm'>{ns.name}</div>
                <div className='truncate text-muted-foreground text-xs capitalize'>
                  {ns.owner_type} &middot; {ns.slug}
                </div>
              </div>
              {canManage && (
                <AlertDialog>
                  <AlertDialogTrigger asChild>
                    <Button
                      variant='ghost'
                      size='icon'
                      disabled={deleteNamespace.isPending}
                      aria-label={`Disconnect ${ns.name}`}
                    >
                      <Trash2 className='h-4 w-4 text-muted-foreground' />
                    </Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>Disconnect "{ns.name}"?</AlertDialogTitle>
                      <AlertDialogDescription>
                        This will remove the GitHub connection for this organization. Workspaces
                        linked to repositories under this connection will lose access until it is
                        re-added.
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>Cancel</AlertDialogCancel>
                      <AlertDialogAction
                        className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
                        onClick={() => handleDelete(ns.id, ns.name)}
                      >
                        Disconnect
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className='flex flex-col items-center justify-center gap-3 rounded-lg border border-border border-dashed py-10 text-center'>
          <div className='flex h-10 w-10 items-center justify-center rounded-full bg-muted'>
            <GithubIcon className='h-5 w-5 text-muted-foreground' />
          </div>
          <p className='text-muted-foreground text-sm'>No GitHub connections yet.</p>
        </div>
      )}
    </div>
  );
}
