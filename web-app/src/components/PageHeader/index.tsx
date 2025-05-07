import { cn } from "@/libs/shadcn/utils";
import { SidebarTrigger, useSidebar } from "@/components/ui/shadcn/sidebar";
const PageHeader = ({
  className,
  children,
  ...props
}: React.ComponentProps<"div">) => {
  const { open } = useSidebar();
  return (
    <div className={cn("flex gap-2 p-2", className)} {...props}>
      {!open && <SidebarTrigger />}
      {children}
    </div>
  );
};

export default PageHeader;
