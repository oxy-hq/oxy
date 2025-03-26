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

const Threads = () => {
  const location = useLocation();
  const { data: threads } = useThreads();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const isThreadsPage = location.pathname === "/threads";

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
          {threads?.map((thread) => (
            <ThreadItem key={thread.id} thread={thread} />
          ))}
        </SidebarMenuSub>
      </SidebarMenuItem>
    </>
  );
};

export default Threads;
