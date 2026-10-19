import { cn } from "@/libs/shadcn/utils";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { LifecycleVerdict } from "../../appStatus";

/**
 * The verdict, as every surface draws it: a dot in the verdict's tone and its label.
 *
 * `compact` is the fleet row's and the partner table's form — no ring, no pill
 * surface, so forty of them in a column read as a column rather than as forty badges.
 * The console header and the card use the full pill.
 */
export const LifecycleVerdictPill = ({
  verdict,
  compact,
  className,
  "data-testid": dataTestId
}: {
  verdict: LifecycleVerdict;
  compact?: boolean;
  className?: string;
  "data-testid"?: string;
}) => {
  if (!compact) {
    return (
      <AdminStatusPill
        tone={verdict.tone}
        label={verdict.label}
        className={className}
        data-testid={dataTestId}
      />
    );
  }
  const tone = ADMIN_TONE[verdict.tone];
  return (
    <span
      className={cn("flex items-center gap-1.5", className)}
      data-testid={dataTestId}
      data-verdict={verdict.status ?? "unknown"}
    >
      <span className={cn("size-1.5 shrink-0 rounded-full", tone.dot)} aria-hidden />
      <span className={cn("truncate", tone.text)}>{verdict.label}</span>
    </span>
  );
};
