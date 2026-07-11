import type { ReactNode } from "react";
import { cn } from "@/libs/shadcn/utils";

export interface KeyValueRow {
  label: string;
  value: ReactNode;
  tone?: "default" | "error";
}

/** Compact monospace definition grid used across the inspector tabs. */
export function KeyValueGrid({ rows }: { rows: KeyValueRow[] }) {
  return (
    <dl className='grid grid-cols-[7.5rem_1fr] gap-x-3 gap-y-1.5 font-mono text-xs'>
      {rows.map((row) => (
        <div key={row.label} className='contents'>
          <dt className='text-muted-foreground'>{row.label}</dt>
          <dd
            className={cn(
              "m-0 break-words tabular-nums",
              row.tone === "error" && "text-destructive"
            )}
          >
            {row.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
