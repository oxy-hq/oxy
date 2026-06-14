import { Badge } from "@/components/ui/shadcn/badge";
import { cn } from "@/libs/shadcn/utils";

import { statusAccent } from "../utils";

/**
 * Compile status pill. Colour maps to the FailureKind taxonomy via the
 * shared `statusAccent` token convention (emerald=ready, destructive=
 * failed, amber=compiling) so the flat list and the workspace rollup
 * read identically.
 */
export const StatusBadge = ({
  status,
  className
}: {
  status: string | null | undefined;
  className?: string;
}) => (
  <Badge className={cn(statusAccent(status), "px-1.5 py-0 text-[10px]", className)}>
    {status ?? "—"}
  </Badge>
);
