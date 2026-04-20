import { Check, ChevronDown, FolderOpen } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Separator } from "@/components/ui/shadcn/separator";
import { useAllWorkspaces as useAllProjects } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

export function IDEProjectSwitcher() {
  const [open, setOpen] = useState(false);
  const { workspace: currentProject } = useCurrentWorkspace();
  const { data: projects = [] } = useAllProjects();
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const handleSelect = (projectId: string) => {
    if (projectId === currentProject?.id) {
      setOpen(false);
      return;
    }
    const target = projects.find((p) => p.id === projectId);
    if (!target?.org_id) return;
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).ROOT);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type='button'
          className='flex h-7 max-w-44 items-center gap-1.5 rounded border border-border/50 bg-transparent px-2 text-sm transition-colors hover:border-border hover:bg-accent/40'
          aria-label='Switch workspace'
        >
          <FolderOpen className='h-3.5 w-3.5 shrink-0 text-muted-foreground/60' />
          <span className='min-w-0 flex-1 truncate text-left text-muted-foreground text-xs'>
            {currentProject?.name ?? "…"}
          </span>
          <ChevronDown className='h-3 w-3 shrink-0 text-muted-foreground/40' />
        </button>
      </PopoverTrigger>
      <PopoverContent side='bottom' align='start' sideOffset={4} className='w-52 p-1.5'>
        <p className='px-2 pb-1 font-medium text-[11px] text-muted-foreground uppercase tracking-wide'>
          Switch workspace
        </p>
        {projects.map((p) => (
          <button
            key={p.id}
            type='button'
            onClick={() => handleSelect(p.id)}
            className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent'
          >
            <span
              className={`flex-1 truncate text-sm ${p.id === currentProject?.id ? "font-medium text-foreground" : "text-muted-foreground"}`}
            >
              {p.name}
            </span>
            {p.id === currentProject?.id ? <Check className='h-3 w-3 text-primary' /> : null}
          </button>
        ))}
        <Separator className='my-1' />
        <button
          type='button'
          onClick={() => {
            setOpen(false);
            navigate(ROUTES.ORG(orgSlug).WORKSPACES);
          }}
          className='flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground text-sm transition-colors hover:bg-accent hover:text-foreground'
        >
          Manage workspaces
        </button>
      </PopoverContent>
    </Popover>
  );
}
