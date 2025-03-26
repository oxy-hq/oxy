import { MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { ThreadItem } from "@/types/chat";
import {
  SidebarMenuAction,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { useCallback } from "react";
import useDeleteThread from "@/hooks/api/useDeleteThread";
import { cn } from "@/libs/shadcn/utils";

interface ItemProps {
  thread: ThreadItem;
}

const Item = ({ thread }: ItemProps) => {
  const navigate = useNavigate();
  const location = useLocation();
  const { mutate: deleteThread } = useDeleteThread();

  const isActive = location.pathname === `/threads/${thread.id}`;

  const handleDeleteThread = useCallback(() => {
    const threadId = thread.id;
    deleteThread(threadId, {
      onSuccess: () => {
        if (isActive) {
          navigate("/threads");
        }
      },
    });
  }, [deleteThread, isActive, navigate, thread.id]);

  return (
    <SidebarMenuSubItem key={thread.id}>
      <SidebarMenuSubButton asChild isActive={isActive}>
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
            className="cursor-pointer"
            onSelect={handleDeleteThread}
          >
            <Trash2 className="text-destructive" />
            <span className="text-destructive">Delete</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </SidebarMenuSubItem>
  );
};

export default Item;
