import { Workflow } from "lucide-react";
import { useState } from "react";
import { Link, useLocation } from "react-router-dom";

import LoadingSkeleton from "@/components/ui/LoadingSkeleton";
import { Button } from "@/components/ui/shadcn/button";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import { useAgenticWorkflowFiles } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import type { WorkflowFile } from "@/services/api/agenticWorkflows";
import useCurrentOrg from "@/stores/useCurrentOrg";

const PREVIEW_LIMIT = 5;

/** Hook wrapper — fetches files and trims them to the visible window. */
const useSidebarWorkflows = (showAll: boolean) => {
  const { data, isPending } = useAgenticWorkflowFiles();
  const all = data ?? [];
  return {
    files: showAll ? all : all.slice(0, PREVIEW_LIMIT),
    total: all.length,
    isPending
  };
};

/** Derive a display name from a relative path (strip dir + known suffix). */
const displayName = (path: string): string => {
  const stem = path.split("/").pop() ?? path;
  return stem.replace(/\.(workflow|procedure|automation)\.ya?ml$/i, "");
};

/** Pure presentational view — receives data, renders nothing else. */
const WorkflowsListView = ({
  files,
  total,
  isPending,
  showAll,
  onToggleShowAll,
  hrefFor,
  activeHref
}: {
  files: WorkflowFile[];
  total: number;
  isPending: boolean;
  showAll: boolean;
  onToggleShowAll: () => void;
  hrefFor: (file: WorkflowFile) => string;
  activeHref?: string;
}) => (
  <SidebarMenuItem>
    <SidebarMenuButton asChild>
      <div>
        <Workflow />
        <span>Procedures</span>
      </div>
    </SidebarMenuButton>
    <SidebarMenuSub className='ml-[15px]'>
      {isPending && <LoadingSkeleton variant='inline' />}
      {!isPending &&
        files.map((file) => {
          const href = hrefFor(file);
          const name = displayName(file.path);
          return (
            <SidebarMenuSubItem key={file.path_b64}>
              <SidebarMenuSubButton asChild isActive={activeHref === href}>
                <Link to={href} data-testid={`workflow-link-${name}`}>
                  <span>{name}</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          );
        })}
      {total > PREVIEW_LIMIT && (
        <Button
          size='sm'
          variant='ghost'
          onClick={onToggleShowAll}
          className='w-full py-1 text-left text-muted-foreground text-sm hover:text-foreground'
        >
          {showAll ? "Show less" : `Show all (${total} procedures)`}
        </Button>
      )}
    </SidebarMenuSub>
  </SidebarMenuItem>
);

export function Workflows() {
  const [showAll, setShowAll] = useState(false);
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { files, total, isPending } = useSidebarWorkflows(showAll);

  return (
    <WorkflowsListView
      files={files}
      total={total}
      isPending={isPending}
      showAll={showAll}
      onToggleShowAll={() => setShowAll((v) => !v)}
      hrefFor={(file) => ROUTES.ORG(orgSlug).WORKSPACE(project.id).WORKFLOW(file.path_b64).ROOT}
      activeHref={location.pathname}
    />
  );
}
