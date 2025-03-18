import { cn } from "@/libs/shadcn/utils";
import { SidebarTrigger } from "../ui/shadcn/sidebar";
const PageHeader = ({
  className,
  children,
  ...props
}: React.ComponentProps<"div">) => {
  return (
    <div className={cn("flex gap-2 p-2", className)} {...props}>
      <SidebarTrigger className="md:hidden" />
      {children}
    </div>
  );
};

export default PageHeader;
