import { PlusCircle, Search } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { useWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import type { Workspace } from "@/types/workspace";
import Header from "./components/Header";
import Content from "./content";

export default function WorkspacesPage() {
  const navigate = useNavigate();
  const [searchQuery, setSearchQuery] = useState("");

  const { data: workspacesData, isLoading, error, refetch } = useWorkspaces();

  const filteredWorkspaces =
    workspacesData?.workspaces?.filter((workspace) =>
      workspace.name.toLowerCase().includes(searchQuery.toLowerCase())
    ) || [];

  const handleWorkspaceClick = (workspace: Workspace) => {
    if (!workspace.project) return;
    navigate(ROUTES.PROJECT(workspace.project?.id).ROOT);
  };

  const handleClearSearch = () => setSearchQuery("");

  return (
    <div className='customScrollbar flex w-full flex-col overflow-auto'>
      <Header />

      <div className='mx-auto w-full max-w-6xl max-w-[1200px] flex-1 p-6'>
        <div className='mb-6'>
          <h1 className='mb-4 text-2xl'>Your Workspaces</h1>
          <div className='flex items-center justify-between'>
            <div className='flex items-center'>
              {workspacesData?.workspaces && workspacesData.workspaces.length > 0 && (
                <div className='relative'>
                  <Search className='absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 transform text-muted-foreground' />
                  <Input
                    placeholder='Search for a workspace'
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className='w-80 pl-10'
                  />
                </div>
              )}
            </div>
            <Button onClick={() => navigate(ROUTES.WORKSPACE.CREATE_WORKSPACE)}>
              <PlusCircle className='mr-2 h-4 w-4' />
              New Workspace
            </Button>
          </div>
        </div>

        <div className='space-y-6'>
          <Content
            workspaces={workspacesData?.workspaces}
            filteredWorkspaces={filteredWorkspaces}
            searchQuery={searchQuery}
            isLoading={isLoading}
            error={error}
            onWorkspaceClick={handleWorkspaceClick}
            onClearSearch={handleClearSearch}
            onRetry={refetch}
          />
        </div>
      </div>
    </div>
  );
}
