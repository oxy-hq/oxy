import { cn } from "@/libs/shadcn/utils";

/**
 * Padded header row for page-level actions.
 *
 * Returns `null` when there are no children — this is intentional so the
 * page doesn't reserve `p-2` of empty vertical space. Any `className`,
 * `data-*`, or other props are dropped in that case, so don't rely on
 * side-effects from passing props alone — only the rendered output. If a
 * future caller needs the wrapper to render even without children, gate
 * the early return on those props explicitly.
 */
const PageHeader = ({ className, children, ...props }: React.ComponentProps<"div">) => {
  if (!children) return null;
  return (
    <div className={cn("flex gap-2 p-2", className)} {...props}>
      {children}
    </div>
  );
};

export default PageHeader;
