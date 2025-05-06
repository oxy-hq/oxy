import { FileCheck2, MoreHorizontal, Trash2 } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { useCallback, useState } from "react";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  SidebarMenuAction,
} from "@/components/ui/shadcn/sidebar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import useTasks from "@/hooks/api/useTasks";
import useDeleteTask from "@/hooks/api/useDeleteTask";
import { cn } from "@/libs/shadcn/utils";
import ClearAllTasksDialog from "./Tasks/ClearAllTasksDialog";

export function Tasks() {
  const location = useLocation();
  const { data: tasks } = useTasks();
  const { mutate: deleteTask } = useDeleteTask();
  const [confirmOpen, setConfirmOpen] = useState(false);

  const handleDeleteTask = useCallback(
    (taskId: string) => {
      deleteTask(taskId);
    },
    [deleteTask],
  );

  return (
    <>
      <ClearAllTasksDialog open={confirmOpen} onOpenChange={setConfirmOpen} />
      <SidebarMenuItem>
        <SidebarMenuButton asChild>
          <div>
            <FileCheck2 />
            <span>Tasks</span>
          </div>
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
              <span className="text-destructive">Clear all tasks</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <SidebarMenuSub>
          {tasks &&
            tasks.map((task) => {
              const taskUri = `/tasks/${task.id}`;
              const isActive = location.pathname === taskUri;
              return (
                <SidebarMenuSubItem key={task.id}>
                  <SidebarMenuSubButton asChild isActive={isActive}>
                    <Link to={taskUri}>
                      <span>{task.title}</span>
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
                        onSelect={() => handleDeleteTask(task.id)}
                      >
                        <Trash2 className="text-destructive" />
                        <span className="text-destructive">Delete</span>
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </SidebarMenuSubItem>
              );
            })}
        </SidebarMenuSub>
      </SidebarMenuItem>
    </>
  );
}
