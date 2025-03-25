import { MessagesSquare, MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import useThreads from "@/hooks/api/useThreads";
import useDeleteThread from "@/hooks/api/useDeleteThread";
import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "../ui/shadcn/dropdown-menu";
import { DropdownMenuItem } from "@radix-ui/react-dropdown-menu";
import { Button } from "../ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";

const Threads = () => {
  const location = useLocation();
  const { data: threads } = useThreads();
  const navigate = useNavigate();
  const isThreads = location.pathname === "/threads";

  const { mutate: deleteThread } = useDeleteThread();

  const handleDeleteThread = useCallback(
    (threadId: string) => {
      deleteThread(threadId, {
        onSuccess: () => {
          if (location.pathname === `/threads/${threadId}`) {
            navigate("/threads");
          }
        },
      });
    },
    [deleteThread, location.pathname, navigate],
  );

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isThreads}>
        <Link to="/threads">
          <MessagesSquare />
          <span>Threads</span>
        </Link>
      </SidebarMenuButton>
      <SidebarMenuSub>
        {threads?.map((thread) => (
          <SidebarMenuSubItem key={thread.id}>
            <SidebarMenuSubButton
              asChild
              isActive={location.pathname === `/threads/${thread.id}`}
            >
              <Link to={`/threads/${thread.id}`}>
                <span>{thread.title}</span>
              </Link>
            </SidebarMenuSubButton>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuAction
                  className={cn(
                    "peer-data-[active=true]/menu-button:text-sidebar-accent-foreground",
                    "group-focus-within/menu-sub-item:opacity-100 group-hover/menu-sub-item:opacity-100",
                    "data-[state=open]:opacity-100 md:opacity-0",
                  )}
                >
                  <MoreHorizontal />
                </SidebarMenuAction>
              </DropdownMenuTrigger>
              <DropdownMenuContent side="bottom" align="start">
                <DropdownMenuItem
                  onSelect={() => handleDeleteThread(thread.id)}
                >
                  <Button variant="ghost" className="w-full">
                    <Trash2 className="text-destructive" />
                    <span className="text-destructive">Delete</span>
                  </Button>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuSubItem>
        ))}
      </SidebarMenuSub>
    </SidebarMenuItem>
  );
};

export default Threads;
