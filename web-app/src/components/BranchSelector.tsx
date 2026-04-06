import { AlertCircle, GitBranch } from "lucide-react";
import * as React from "react";
import { Alert, AlertDescription } from "@/components/ui/shadcn/alert";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "@/components/ui/shadcn/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useProjectBranches } from "@/hooks/api/projects/useProjects";
import { cn } from "@/libs/shadcn/utils";
import useCurrentProject from "@/stores/useCurrentProject";

interface Props {
  selectedBranch: string;
  setSelectedBranch: (branch: string) => void;
}

const BranchSelector = ({ selectedBranch, setSelectedBranch }: Props) => {
  const { project } = useCurrentProject();
  const { data: branchResponse, isLoading, error } = useProjectBranches(project?.id || "");
  const [open, setOpen] = React.useState(false);
  const [inputValue, setInputValue] = React.useState("");

  const branches = branchResponse?.branches || [];
  const activeBranchName = project?.active_branch?.name;

  if (isLoading) {
    return (
      <div className='flex items-center gap-2 rounded-md border bg-muted/30 p-3'>
        <Spinner className='text-muted-foreground' />
      </div>
    );
  }

  if (error) {
    return (
      <Alert variant='destructive'>
        <AlertCircle className='h-4 w-4' />
        <AlertDescription>Failed to load branches. Please try again later.</AlertDescription>
      </Alert>
    );
  }

  const trimmed = inputValue.trim();
  const showCreate = trimmed.length > 0 && !branches.some((b) => b.name === trimmed);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant='outline'
          role='combobox'
          aria-expanded={open}
          className='w-full justify-between bg-input/30'
        >
          <span className='flex items-center gap-2'>
            <GitBranch className='h-4 w-4 shrink-0 text-muted-foreground' />
            {selectedBranch || "Select a branch"}
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-[--radix-popover-trigger-width] p-0'>
        <Command>
          <CommandInput
            placeholder='Search or create branch…'
            value={inputValue}
            onValueChange={setInputValue}
          />
          <CommandList>
            {!showCreate && branches.length === 0 && (
              <CommandEmpty>No branches found.</CommandEmpty>
            )}
            <CommandGroup>
              {branches.map((branch) => (
                <CommandItem
                  key={branch.name}
                  value={branch.name}
                  onSelect={() => {
                    setSelectedBranch(branch.name);
                    setOpen(false);
                    setInputValue("");
                  }}
                >
                  <GitBranch
                    className={cn(
                      "mr-2 h-4 w-4",
                      branch.name === selectedBranch ? "opacity-100" : "opacity-30"
                    )}
                  />
                  <span className='font-medium text-sm'>{branch.name}</span>
                  <div className='ml-2 flex gap-1'>
                    {branch.name === activeBranchName && (
                      <Badge variant='secondary' className='px-1.5 py-0.5 text-xs'>
                        active
                      </Badge>
                    )}
                    {branch.name === selectedBranch && branch.name !== activeBranchName && (
                      <Badge
                        variant='outline'
                        className='border-info/30 bg-info/10 px-1.5 py-0.5 text-info text-xs'
                      >
                        current
                      </Badge>
                    )}
                  </div>
                </CommandItem>
              ))}
              {showCreate && (
                <CommandItem
                  value={`__create__:${trimmed}`}
                  onSelect={() => {
                    setSelectedBranch(trimmed);
                    setOpen(false);
                    setInputValue("");
                  }}
                  className='text-primary'
                >
                  <GitBranch className='mr-2 h-4 w-4' />
                  Create branch &ldquo;<strong>{trimmed}</strong>&rdquo;
                </CommandItem>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
};

export default BranchSelector;
