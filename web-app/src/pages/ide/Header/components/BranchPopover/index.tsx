import { Plus } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
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
import { cn } from "@/libs/shadcn/utils";
import { BranchRow, type BranchRowData } from "./BranchRow";
import { sanitizeBranchName } from "./sanitizeBranchName";

interface Props {
  trigger: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  branches: BranchRowData[];
  activeBranch: string | undefined;
  /** When non-null, that branch is mid-switch; blocks close + dims other rows. */
  switchingTo: string | null;
  isLoading: boolean;
  onSelect: (branchName: string) => void;
  onDelete?: (branchName: string) => void;
  emptyMessage?: string;
}

// Pure shell: props in, JSX out — no API hooks. Each surface
// (workspace switcher, linked-repo picker) wraps this with its own data.
export function BranchPopover({
  trigger,
  open: controlledOpen,
  onOpenChange: controlledOnOpenChange,
  branches,
  activeBranch,
  switchingTo,
  isLoading,
  onSelect,
  onDelete,
  emptyMessage = "No branches found."
}: Props) {
  const [internalOpen, setInternalOpen] = useState(false);
  const [inputValue, setInputValue] = useState("");

  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : internalOpen;
  const setOpen = isControlled ? (controlledOnOpenChange ?? setInternalOpen) : setInternalOpen;

  useEffect(() => {
    if (!open) setInputValue("");
  }, [open]);

  const sanitized = sanitizeBranchName(inputValue);
  const branchNames = branches.map((b) => b.name);
  const showCreate = sanitized.length > 0 && !branchNames.includes(sanitized);

  const handleOpenChange = (next: boolean) => {
    // Block close while a switch is in flight — dismissing mid-switch
    // leaves the user with no spinner to watch.
    if (!next && switchingTo !== null) return;
    setOpen(next);
  };

  const handleSelect = (name: string) => {
    if (switchingTo !== null) return;
    onSelect(name);
  };

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        className='w-72 p-0 shadow-lg'
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
                  <CommandEmpty>{emptyMessage}</CommandEmpty>
                )}
                {branches.length > 0 && (
                  <CommandGroup heading='Branches'>
                    {branches.map((row) => (
                      <BranchRow
                        key={row.name}
                        row={row}
                        isActive={row.name === activeBranch}
                        isSwitchingThis={switchingTo === row.name}
                        isSwitchingOther={switchingTo !== null && switchingTo !== row.name}
                        onSelect={() => handleSelect(row.name)}
                        onDelete={onDelete && row.canDelete ? () => onDelete(row.name) : undefined}
                      />
                    ))}
                  </CommandGroup>
                )}
                {showCreate && (
                  <>
                    <CommandSeparator />
                    <CommandGroup>
                      <CommandItem
                        value={`__create__:${sanitized}`}
                        disabled={switchingTo !== null}
                        onSelect={() => handleSelect(sanitized)}
                        className={cn(
                          "flex min-w-0 cursor-pointer items-center font-mono text-primary text-sm",
                          switchingTo !== null && switchingTo !== sanitized && "opacity-50"
                        )}
                      >
                        {switchingTo === sanitized ? (
                          <Spinner
                            className='mr-1.5 size-3.5 shrink-0'
                            aria-label='Creating branch…'
                          />
                        ) : (
                          <Plus className='mr-1.5 h-3.5 w-3.5 shrink-0' />
                        )}
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
}
