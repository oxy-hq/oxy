import { cn } from "@/libs/shadcn/utils";
import { SidebarTrigger, useSidebar } from "@/components/ui/shadcn/sidebar";
import { useMediaQuery } from "usehooks-ts";
const PageHeader = ({
  className,
  children,
  ...props
}: React.ComponentProps<"div">) => {
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
