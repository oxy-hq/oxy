import { Eye, EyeOff, Loader2 } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

/** The shape both surfaces agree on: a schema, and whether analytics reads it. */
export type OltpSchemaChipData = {
  schema: string;
  kind: string;
  analytics_visible: boolean;
};

/**
 * A tenant's schemas, as a strip of chips.
 *
 * **This is the OLTP model rendered directly.** One database holds many
 * mutually-blind schemas with one read-only analyst above them, and the fact an
 * operator actually needs — *can analytics read this app's live rows?* — used
 * to be invisible: the fleet list showed `2`, a count, in a column wide enough
 * for the names. A filled chip means the analyst can read it, an outlined one
 * means it is hidden. Same width, the security-relevant answer instead of a
 * cardinality.
 *
 * Monospace because these are schema-qualified identifiers an operator types
 * into `psql`, not prose.
 *
 * Read-only on the fleet list; pass `onToggle` on the org page and each chip
 * becomes the control that changes what it displays.
 */
export const OltpSchemaStrip = ({
  schemas,
  onToggle,
  pendingSchema,
  className,
  testIdPrefix = "admin-oltp-schema"
}: {
  schemas: OltpSchemaChipData[];
  /** Omit for a read-only strip. */
  onToggle?: (s: OltpSchemaChipData) => void;
  /** Schema name currently being toggled, so only ITS chip shows a spinner. */
  pendingSchema?: string;
  className?: string;
  testIdPrefix?: string;
}) => {
  if (schemas.length === 0) {
    return <span className='text-muted-foreground text-xs'>no writers</span>;
  }
  return (
    <div className={cn("flex flex-wrap items-center gap-1", className)}>
      {schemas.map((s) => {
        const busy = pendingSchema === s.schema;
        const chip = (
          <>
            {busy ? (
              <Loader2 className='size-2.5 shrink-0 animate-spin' />
            ) : s.analytics_visible ? (
              <Eye className='size-2.5 shrink-0' />
            ) : (
              <EyeOff className='size-2.5 shrink-0' />
            )}
            <span className='truncate'>{s.schema}</span>
          </>
        );
        // Fill carries visibility. `bg-primary/10 + text-primary` is the same
        // pair `AppMark` uses for its monogram, so a visible schema reads as a
        // first-class object rather than as a highlight.
        const tone = s.analytics_visible
          ? "border-primary/25 bg-primary/10 text-primary"
          : "border-border bg-transparent text-muted-foreground";
        const shared =
          "inline-flex max-w-[14rem] items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-xs leading-none";

        if (!onToggle) {
          return (
            <span
              key={s.schema}
              className={cn(shared, tone)}
              title={`${s.schema} · ${s.kind} · analytics ${s.analytics_visible ? "can read it" : "cannot read it"}`}
              data-testid={`${testIdPrefix}-${s.schema}`}
            >
              {chip}
            </span>
          );
        }
        return (
          <button
            key={s.schema}
            type='button'
            disabled={busy}
            onClick={() => onToggle(s)}
            title={
              s.analytics_visible
                ? `Hide ${s.schema} from analytics`
                : `Let analytics read ${s.schema}`
            }
            className={cn(
              shared,
              tone,
              "transition-colors hover:border-primary/40 hover:bg-primary/15 disabled:opacity-60"
            )}
            data-testid={`${testIdPrefix}-toggle-${s.schema}`}
          >
            {chip}
          </button>
        );
      })}
    </div>
  );
};
