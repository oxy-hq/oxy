import { useState } from "react";
import { MessagesSquare, MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation, useParams } from "react-router-dom";
import {
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
} from "@/components/ui/shadcn/sidebar";
import useThreads from "@/hooks/api/threads/useThreads";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import ThreadItem from "./Item";
import ClearAllThreadsDialog from "./ClearAllThreadsDialog";
import { Button } from "@/components/ui/shadcn/button";
import ItemsSkeleton from "../ItemsSkeleton";
import ROUTES from "@/libs/utils/routes";

const SIDEBAR_THREADS_LIMIT = 50;

const Threads = () => {
  const location = useLocation();
  const { projectId } = useParams();
  const { data: threadsResponse, isLoading } = useThreads(
    1,
    SIDEBAR_THREADS_LIMIT,
  );
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [showAll, setShowAll] = useState(false);

  if (!projectId) {
    return null;
  }

  const threadsUri = ROUTES.PROJECT(projectId).THREADS;
  const isThreadsPage = location.pathname === threadsUri;

  const threads = threadsResponse?.threads ?? [];
  const sortedThreads = threads?.sort((a, b) => {
    if (a.created_at && b.created_at) {
      return (
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      );
    }
    return 0;
  });

  const visibleThreads = showAll ? sortedThreads : sortedThreads?.slice(0, 5);

  return (
    <>
      <ClearAllThreadsDialog open={confirmOpen} onOpenChange={setConfirmOpen} />
      <SidebarMenuItem>
        <SidebarMenuButton asChild isActive={isThreadsPage}>
          <Link to={threadsUri}>
            <MessagesSquare />
            <span>Threads</span>
          </Link>
        </SidebarMenuButton>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuAction showOnHover>
              <MoreHorizontal />
            </SidebarMenuAction>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="bottom" align="start">
            <DropdownMenuItem onSelect={() => setConfirmOpen(true)}>
              <Trash2 className="text-destructive" />
              <span className="text-destructive">Clear all threads</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <SidebarMenuSub>
          {isLoading && <ItemsSkeleton />}

          {!isLoading &&
            visibleThreads?.map((thread) => (
              <ThreadItem key={thread.id} thread={thread} />
            ))}

          {!isLoading && threads && threads.length > 5 && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowAll(!showAll)}
              className="w-full text-sm text-muted-foreground hover:text-foreground py-1 text-left"
            >
              {showAll ? "Show less" : `Show ${threads.length} recent threads`}
            </Button>
          )}
        </SidebarMenuSub>
      </SidebarMenuItem>
    </>
  );
};

export default Threads;
