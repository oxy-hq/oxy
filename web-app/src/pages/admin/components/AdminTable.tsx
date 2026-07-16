import type { ReactNode } from "react";
import { TableHead } from "@/components/ui/shadcn/table";
import { cn } from "@/libs/shadcn/utils";

/**
 * The operator-console table treatment, shared so the admin org / user /
 * workspace lists read identically (previously each hand-rolled its own header
 * + row styling and drifted). `AdminTh` is the dense uppercase micro-label
 * header cell; the class constants style the header + clickable body rows.
 */
export function AdminTh({
  children,
  align = "left",
  className
}: {
  children: ReactNode;
  align?: "left" | "right";
  className?: string;
}) {
  return (
    <TableHead
      className={cn(
        "font-medium text-[10px] text-muted-foreground uppercase tracking-wider",
        align === "right" && "text-right",
        className
      )}
    >
      {children}
    </TableHead>
  );
}

/** Header row: a subtle divider under the column labels. */
export const ADMIN_HEADER_ROW_CLASS = "border-border/60";

/** Clickable body row: subtle divider + hover, cursor affordance. */
export const ADMIN_ROW_CLASS =
  "cursor-pointer border-border/60 transition-colors hover:bg-muted/40";
