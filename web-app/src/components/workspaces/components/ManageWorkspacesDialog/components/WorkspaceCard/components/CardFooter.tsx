import { Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { CanOrgAdmin } from "@/components/auth/Can";
import { ConfirmDeleteDialog } from "@/components/ui/ConfirmDeleteDialog";
import { Button } from "@/components/ui/shadcn/button";
import type { WorkspaceSummary } from "@/services/api/workspaces";

type Props = {
  workspace: WorkspaceSummary;
  createdAt: string | null;
  showRename: boolean;
  isDeleting: boolean;
  onRename: () => void;
  onDelete: () => void;
};

export function CardFooter({
  workspace,
  createdAt,
  showRename,
  isDeleting,
  onRename,
  onDelete
}: Props) {
  const timestamp = workspace.git_updated_at
    ? `Updated ${workspace.git_updated_at}`
    : createdAt
      ? `Created ${createdAt}`
      : "";

  return (
    <div className='flex items-center justify-between border-border/40 border-t px-5 py-2.5'>
      <div className='flex flex-col gap-0.5'>
        <span className='text-muted-foreground/50 text-xs'>{timestamp}</span>
        {workspace.created_by_name && (
          <span className='text-muted-foreground/35 text-xs'>by {workspace.created_by_name}</span>
        )}
      </div>

      <CanOrgAdmin>
        <div className='flex items-center gap-1'>
          {showRename && (
            <Button
              variant='ghost'
              size='icon'
              onClick={onRename}
              aria-label={`Rename ${workspace.name}`}
              className='h-6 w-6 text-muted-foreground/30 opacity-0 transition-all hover:bg-muted hover:text-foreground group-hover:opacity-100'
            >
              <Pencil className='size-3' />
            </Button>
          )}
          <DeleteAction workspaceName={workspace.name} disabled={isDeleting} onConfirm={onDelete} />
        </div>
      </CanOrgAdmin>
    </div>
  );
}

function DeleteAction({
  workspaceName,
  disabled,
  onConfirm
}: {
  workspaceName: string;
  disabled: boolean;
  onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);

  const handleConfirm = () => {
    onConfirm();
    setOpen(false);
  };

  return (
    <>
      <Button
        variant='ghost'
        size='icon'
        disabled={disabled}
        onClick={() => setOpen(true)}
        aria-label={`Delete ${workspaceName}`}
        className='h-6 w-6 text-muted-foreground/30 opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100'
      >
        <Trash2 className='size-3.5' />
      </Button>
      <ConfirmDeleteDialog
        open={open}
        onOpenChange={setOpen}
        title='Delete this entire workspace permanently?'
        description='This action cannot be undone. This will permanently delete the workspace, including all pages and files. Please type the name of the workspace to confirm.'
        confirmationName={workspaceName}
        confirmButtonLabel='Permanently delete workspace'
        onConfirm={handleConfirm}
        isPending={disabled}
      />
    </>
  );
}
