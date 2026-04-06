import { GitBranch, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator
} from "@/components/ui/shadcn/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  useDeleteBranch,
  useProjectBranches,
  useSwitchProjectBranch
} from "@/hooks/api/projects/useProjects";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useIdeBranch from "@/stores/useIdeBranch";

interface BranchQuickSwitcherProps {
  trigger: React.ReactNode;
  // Controlled open state — lets an external button also open this popover
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export const BranchQuickSwitcher = ({
  trigger,
  open: controlledOpen,
  onOpenChange: controlledOnOpenChange
}: BranchQuickSwitcherProps) => {
  const [internalOpen, setInternalOpen] = useState(false);
  const [inputValue, setInputValue] = useState("");
  const navigate = useNavigate();
  const { project, branchName: currentBranch } = useCurrentProjectBranch();
  const { setCurrentBranch } = useIdeBranch();
  const { data: branchResponse, isLoading } = useProjectBranches(project?.id || "");
  const switchBranch = useSwitchProjectBranch();

  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : internalOpen;
  const setOpen = isControlled ? (controlledOnOpenChange ?? setInternalOpen) : setInternalOpen;

  const projectId = project?.id || "";
  const deleteBranch = useDeleteBranch(projectId);
  const branches = branchResponse?.branches || [];
  const activeBranchName = project?.active_branch?.name;
  const trimmed = inputValue.trim();
  // Sanitize to a valid git branch name: spaces → hyphens, strip invalid chars
  const sanitized = trimmed
    .replace(/\s+/g, "-")
    .replace(/[~^:?*[\\ ]+/g, "")
    .replace(/\.{2,}/g, ".")
    .replace(/^[.-]+/, "")
    .replace(/\.+$/, "")
    .replace(/-+/g, "-");
  const showCreate = sanitized.length > 0 && !branches.some((b) => b.name === sanitized);

  const handleDelete = async (e: React.MouseEvent, branchName: string) => {
    e.stopPropagation();
    if (!confirm(`Delete branch "${branchName}"?`)) return;
    try {
      const result = await deleteBranch.mutateAsync(branchName);
      if (result.success) {
        toast.success(`Branch "${branchName}" deleted`);
        if (branchName === currentBranch) {
          const fallback = branches.find((b) => b.name !== branchName)?.name ?? activeBranchName;
          if (fallback) {
            await switchBranch.mutateAsync({ projectId, branchName: fallback });
            setCurrentBranch(projectId, fallback);
            navigate(ROUTES.PROJECT(projectId).IDE.ROOT);
          }
        }
      } else {
        toast.error(result.message || "Failed to delete branch");
      }
    } catch {
      toast.error("Failed to delete branch");
    }
  };

  const handleSelect = async (branchName: string) => {
    if (branchName === currentBranch) {
      setOpen(false);
      setInputValue("");
      return;
    }
    setOpen(false);
    setInputValue("");
    try {
      await switchBranch.mutateAsync({ projectId, branchName });
      setCurrentBranch(projectId, branchName);
      toast.success(`Switched to "${branchName}"`);
      navigate(ROUTES.PROJECT(projectId).IDE.ROOT);
    } catch {
      toast.error("Failed to switch branch.");
    }
  };

  const handleOpenChange = (next: boolean) => {
    setOpen(next);
    if (!next) setInputValue("");
  };

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        className='w-56 p-0 shadow-lg'
        align='start'
        sideOffset={6}
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        <Command>
          <CommandInput
            placeholder='Switch or create branch…'
            value={inputValue}
            onValueChange={setInputValue}
            // eslint-disable-next-line jsx-a11y/no-autofocus
            autoFocus
          />
          <CommandList>
            {isLoading ? (
              <div className='flex items-center gap-2 px-3 py-4 text-muted-foreground text-sm'>
                <Spinner className='size-3' />
              </div>
            ) : (
              <>
                {!showCreate && branches.length === 0 && (
                  <CommandEmpty>No branches found.</CommandEmpty>
                )}
                {branches.length > 0 && (
                  <CommandGroup heading='Branches'>
                    {branches.map((branch) => (
                      <CommandItem
                        key={branch.name}
                        value={branch.name}
                        onSelect={() => handleSelect(branch.name)}
                        className='group flex cursor-pointer items-center gap-2.5 font-mono text-sm'
                      >
                        <span
                          className={cn(
                            "h-1.5 w-1.5 shrink-0 rounded-full transition-colors",
                            branch.name === currentBranch
                              ? "bg-primary"
                              : "bg-transparent group-aria-selected:bg-muted-foreground/25"
                          )}
                        />
                        <span className='min-w-0 flex-1 truncate'>{branch.name}</span>
                        {branch.name === activeBranchName && branch.name !== currentBranch && (
                          <span className='shrink-0 font-sans text-[10px] text-muted-foreground/60'>
                            active
                          </span>
                        )}
                        {branch.name === currentBranch && (
                          <GitBranch className='h-3 w-3 shrink-0 text-primary' />
                        )}
                        {branch.name !== currentBranch && branch.name !== activeBranchName && (
                          <button
                            type='button'
                            className='ml-auto hidden items-center text-muted-foreground hover:text-destructive group-hover:flex'
                            onClick={(e) => handleDelete(e, branch.name)}
                            title='Delete branch'
                          >
                            <Trash2 className='h-3 w-3' />
                          </button>
                        )}
                      </CommandItem>
                    ))}
                  </CommandGroup>
                )}
                {showCreate && (
                  <>
                    <CommandSeparator />
                    <CommandGroup>
                      <CommandItem
                        value={`__create__:${sanitized}`}
                        onSelect={() => handleSelect(sanitized)}
                        className='flex min-w-0 cursor-pointer items-center font-mono text-primary text-sm'
                      >
                        <Plus className='mr-1.5 h-3.5 w-3.5 shrink-0' />
                        <span className='shrink-0'>Create &ldquo;</span>
                        <strong className='min-w-0 truncate'>{sanitized}</strong>
                        <span className='shrink-0'>&rdquo;</span>
                      </CommandItem>
                    </CommandGroup>
                  </>
                )}
              </>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
};
