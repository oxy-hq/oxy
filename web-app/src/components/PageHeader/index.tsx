import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { cn } from "@/libs/shadcn/utils";

/**
 * Renders the sidebar trigger (when the sidebar is collapsed or on mobile)
 * alongside any header `children` you pass.
 *
 * Returns `null` when there is no trigger to show AND no children — this is
 * intentional so the page doesn't reserve `p-2` of empty vertical space on
 * desktop with the sidebar expanded. Any `className`, `data-*`, or other
 * props are dropped in that case, so don't rely on side-effects from passing
 * props alone — only the rendered output. If a future caller needs the
 * wrapper to render even without trigger or children, gate the early return
 * on those props explicitly.
 */
const PageHeader = ({ className, children, ...props }: React.ComponentProps<"div">) => {
  const { open, isMobile } = useSidebar();
  const showTrigger = !open || isMobile;
  if (!showTrigger && !children) return null;
  return (
    <div className={cn("flex gap-2 p-2", className)} {...props}>
      {showTrigger && <SidebarTrigger />}
      {children}
    </div>
  );
};

export default PageHeader;
