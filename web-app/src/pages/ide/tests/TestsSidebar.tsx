import { Bot, FileText, FlaskConical, History, LayoutDashboard, Plus } from "lucide-react";
import type React from "react";
import { Link, useLocation, useParams } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { FieldError } from "@/components/ui/shadcn/field";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSubButton
} from "@/components/ui/shadcn/sidebar";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useTestFiles from "@/hooks/api/tests/useTestFiles";
import { useCreateTestFile } from "@/hooks/useCreateTestFile";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useTestFileResults from "@/stores/useTestFileResults";
import { EvalEventState } from "@/types/eval";

interface TestsSidebarProps {
  setSidebarOpen: (open: boolean) => void;
}

const TestsSidebar: React.FC<TestsSidebarProps> = ({ setSidebarOpen }) => {
  const location = useLocation();
  const { pathb64: activePathb64 } = useParams();
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { data: testFiles, isLoading } = useTestFiles();
  const store = useTestFileResults();
  const createTestFile = useCreateTestFile();

  const isFileRunning = (pathb64: string, caseCount: number) => {
    for (let i = 0; i < caseCount; i++) {
      const cs = store.getCase(projectId, branchName, pathb64, i);
      if (cs.state === EvalEventState.Started || cs.state === EvalEventState.Progress) return true;
    }
    return false;
  };

  const getFileDisplayName = (file: { path: string; name: string | null }) => {
    if (file.name) return file.name;
    const fileName = file.path.split("/").pop() ?? file.path;
    return fileName.replace(/\.test\.(yml|yaml)$/, "");
  };

  const getFileIcon = (path: string) => {
    if (path.includes(".agent.")) return Bot;
    return FileText;
  };

  const isDashboardActive =
    location.pathname === ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.TESTS.ROOT && !activePathb64;
  const isRunsActive =
    location.pathname === ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.TESTS.RUNS;

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader
        title='Tests'
        onCollapse={() => setSidebarOpen(false)}
        actions={
          <Button
            variant='ghost'
            size='sm'
            onClick={createTestFile.openDialog}
            tooltip={{ content: "New Test File", side: "right" }}
          >
            <Plus className='h-4 w-4' />
          </Button>
        }
      />
      <SidebarContent className='h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='pt-2'>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuSubButton asChild isActive={isDashboardActive}>
                <Link to={ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.TESTS.ROOT}>
                  <LayoutDashboard className='h-4 w-4' />
                  <span>Dashboard</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuSubButton asChild isActive={isRunsActive}>
                <Link to={ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.TESTS.RUNS}>
                  <History className='h-4 w-4' />
                  <span>Runs</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
        <SidebarGroup className='pt-1'>
          <SidebarGroupLabel className='px-2 font-semibold text-muted-foreground text-xs'>
            Agent tests
          </SidebarGroupLabel>
          <SidebarMenu>
            {isLoading &&
              Array.from({ length: 3 }).map((_, i) => (
                <SidebarMenuItem key={i}>
                  <div className='px-2 py-1'>
                    <Skeleton className='h-6 w-full' />
                  </div>
                </SidebarMenuItem>
              ))}
            {testFiles?.map((file) => {
              const pathb64 = encodeBase64(file.path);
              const Icon = getFileIcon(file.path);
              const running = isFileRunning(pathb64, file.case_count);
              return (
                <SidebarMenuItem key={file.path}>
                  <SidebarMenuSubButton asChild isActive={activePathb64 === pathb64}>
                    <Link
                      to={ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.TESTS.TEST_FILE(pathb64)}
                    >
                      {running ? (
                        <Spinner className='text-primary' />
                      ) : (
                        <Icon className='h-4 w-4' />
                      )}
                      <span className='flex-1 truncate'>{getFileDisplayName(file)}</span>
                      <Badge variant='secondary' className='ml-auto text-xs'>
                        {file.case_count}
                      </Badge>
                    </Link>
                  </SidebarMenuSubButton>
                </SidebarMenuItem>
              );
            })}
            {!isLoading && testFiles?.length === 0 && (
              <div className='px-3 py-4 text-center text-muted-foreground text-xs'>
                No test files found. Create a <code>.test.yml</code> file to get started.
              </div>
            )}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>

      <Dialog open={createTestFile.dialogOpen} onOpenChange={createTestFile.setDialogOpen}>
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle className='flex items-center gap-2'>
              <FlaskConical className='h-5 w-5' />
              New Test File
            </DialogTitle>
          </DialogHeader>
          <div className='grid gap-4 py-4'>
            <div className='grid gap-2'>
              <Label htmlFor='testFileName'>Name</Label>
              <div className='flex items-center gap-2'>
                <Input
                  id='testFileName'
                  ref={createTestFile.inputRef}
                  value={createTestFile.fileName}
                  onChange={(e) => {
                    createTestFile.setFileName(e.target.value);
                    createTestFile.setError(null);
                  }}
                  onKeyDown={createTestFile.handleKeyDown}
                  placeholder='my-agent'
                  className={createTestFile.error ? "border-destructive" : ""}
                />
                <span className='whitespace-nowrap text-muted-foreground text-sm'>.test.yml</span>
              </div>
              {createTestFile.error && <FieldError>{createTestFile.error}</FieldError>}
            </div>
          </div>
          <DialogFooter>
            <Button
              variant='outline'
              onClick={() => createTestFile.setDialogOpen(false)}
              disabled={createTestFile.isCreating}
            >
              Cancel
            </Button>
            <Button
              onClick={createTestFile.handleCreate}
              disabled={createTestFile.isCreating || !createTestFile.fileName.trim()}
            >
              {createTestFile.isCreating ? <Spinner /> : "Create"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default TestsSidebar;
