import { useMediaQuery } from "usehooks-ts";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { cn } from "@/libs/shadcn/utils";

const PageHeader = ({ className, children, ...props }: React.ComponentProps<"div">) => {
  const { open } = useSidebar();
  const isMobile = useMediaQuery("(max-width: 767px)");
  return (
    <div className={cn("flex gap-2 p-2", className)} {...props}>
      {(!open || isMobile) && <SidebarTrigger />}
      {children}
    </div>
  );
};

export default PageHeader;
