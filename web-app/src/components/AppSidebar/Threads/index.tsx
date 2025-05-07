import { useState } from "react";
import { MessagesSquare, MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
} from "@/components/ui/shadcn/sidebar";
import useThreads from "@/hooks/api/useThreads";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import ThreadItem from "./Item";
import ClearAllThreadsDialog from "./ClearAllThreadsDialog";
import { Button } from "@/components/ui/shadcn/button";

const Threads = () => {
  const location = useLocation();
  const { data: threads } = useThreads();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [showAll, setShowAll] = useState(false);
  const isThreadsPage = location.pathname === "/threads";

  const visibleThreads = showAll ? threads : threads?.slice(0, 5);

  return (
    <>
      <ClearAllThreadsDialog open={confirmOpen} onOpenChange={setConfirmOpen} />
      <SidebarMenuItem>
        <SidebarMenuButton asChild isActive={isThreadsPage}>
          <Link to="/threads">
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
          {visibleThreads?.map((thread) => (
            <ThreadItem key={thread.id} thread={thread} />
          ))}
          {threads && threads.length > 5 && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowAll(!showAll)}
              className="w-full text-sm text-muted-foreground hover:text-foreground py-1 text-left"
            >
              {showAll ? "Show less" : `Show all (${threads.length} threads)`}
            </Button>
          )}
        </SidebarMenuSub>
      </SidebarMenuItem>
    </>
  );
};

export default Threads;
