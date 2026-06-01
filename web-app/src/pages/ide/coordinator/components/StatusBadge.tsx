import type React from "react";
import { cn } from "@/libs/shadcn/utils";
import { normalizeStatus, RUN_STATUS } from "./constants";

/**
 * Status badge — colored dot + label. The single source of truth for how a
 * run status renders anywhere in the coordinator dashboard.
 */
export const StatusBadge: React.FC<{
  status: string;
  /** Hide the label, render the icon only (dense tables, timeline). */
  iconOnly?: boolean;
  className?: string;
}> = ({ status, iconOnly, className }) => {
  const key = normalizeStatus(status);
  const meta = RUN_STATUS[key];
  const Icon = meta.icon;
  return (
    <span
      className={cn("inline-flex items-center gap-1.5 font-medium text-xs", meta.fg, className)}
      title={meta.label}
    >
      <Icon className={cn("h-3.5 w-3.5 shrink-0", meta.spin && "animate-spin")} />
      {!iconOnly && meta.label}
    </span>
  );
};
