import { MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
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
import useDeleteThread from "@/hooks/api/threads/useDeleteThread";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";

interface ItemProps {
  thread: ThreadItem;
}

const Item = ({ thread }: ItemProps) => {
  const navigate = useNavigate();
  const location = useLocation();
  const { projectId } = useParams();
  const { mutate: deleteThread } = useDeleteThread();

  if (!projectId) {
    throw new Error("Project ID is required");
  }

  const threadUri = ROUTES.PROJECT(projectId).THREAD(thread.id);
  const isActive = location.pathname === threadUri;

  const handleDeleteThread = useCallback(() => {
    const threadId = thread.id;
    deleteThread(threadId, {
      onSuccess: () => {
        if (isActive) {
          navigate(ROUTES.PROJECT(projectId).THREADS);
        }
      },
    });
  }, [deleteThread, isActive, navigate, thread.id, projectId]);

  return (
    <SidebarMenuSubItem key={thread.id}>
      <SidebarMenuSubButton asChild isActive={isActive}>
        <Link to={threadUri}>
          <span className="truncate">{thread.title}</span>
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
