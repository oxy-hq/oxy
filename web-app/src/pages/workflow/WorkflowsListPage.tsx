/**
 * Minimal workflow file picker — landing page when navigating to
 * `/workflows` without a specific path.
 *
 * Lists every `.workflow.yml` / `.procedure.yml` / `.automation.yml` known
 * to the workspace and links each to the run page. No quick-launch from
 * here yet — that will come in a follow-up if it earns its weight.
 */

import { ChevronRight, Workflow as WorkflowIcon } from "lucide-react";
import { Link } from "react-router-dom";
import PageHeader from "@/components/PageHeader";
import ErrorAlert from "@/components/ui/ErrorAlert";
import LoadingSkeleton from "@/components/ui/LoadingSkeleton";
import { useAgenticWorkflowFiles } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const WorkflowsListPage = () => {
  const filesQuery = useAgenticWorkflowFiles();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  return (
    <div className='flex h-full w-full flex-col'>
      <PageHeader className='items-center border-border border-b-1'>
        <div className='flex w-full items-center gap-2'>
          <WorkflowIcon className='size-4' />
          <span className='font-medium text-sm'>Workflows</span>
        </div>
      </PageHeader>

      <div className='flex-1 overflow-auto p-4'>
        {filesQuery.isLoading && <LoadingSkeleton className='h-12 w-full' />}
        {filesQuery.isError && (
          <ErrorAlert message={filesQuery.error?.message ?? "Failed to load workflows"} />
        )}
        {filesQuery.isSuccess && filesQuery.data.length === 0 && (
          <p className='text-muted-foreground'>
            No workflow files found. Create a `.workflow.yml` in the project to get started.
          </p>
        )}
        {filesQuery.isSuccess && filesQuery.data.length > 0 && (
          <ul className='flex flex-col gap-2'>
            {filesQuery.data.map((file) => (
              <li key={file.path_b64}>
                <Link
                  to={ROUTES.ORG(orgSlug).WORKSPACE(project.id).WORKFLOW(file.path_b64).ROOT}
                  className='flex items-center gap-3 rounded-md border bg-card px-3 py-2 transition-colors hover:bg-muted/50'
                >
                  <WorkflowIcon className='size-4 text-muted-foreground' />
                  <span className='flex-1 truncate font-medium text-sm'>{file.path}</span>
                  <ChevronRight className='size-4 text-muted-foreground' />
                </Link>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
};

export default WorkflowsListPage;
