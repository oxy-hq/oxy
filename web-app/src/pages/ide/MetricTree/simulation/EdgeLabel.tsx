import { cn } from "@/libs/shadcn/utils";

/** The backend writes an edge as `driver -> target`; accept the arrow glyph too
 *  so a prettified id never renders as one unsplit run of text. */
function splitEdge(edge: string): [string, string | null] {
  const parts = edge.split(/\s*(?:->|→)\s*/);
  return [parts[0] ?? edge, parts[1] ?? null];
}

/**
 * An edge id on two lines, driver over target.
 *
 * The panel is ~340px at its narrowest and a single qualified measure id is
 * already wider than that, so truncating the pair on one line cut the target
 * away entirely — the half that says what actually moved. Two truncated lines
 * keep both ends legible, and the full string stays in the `title`.
 */
export function EdgeLabel({ edge, className }: { edge: string; className?: string }) {
  const [driver, target] = splitEdge(edge);

  return (
    <div
      className={cn("flex min-w-0 flex-col font-mono text-[10px] text-muted-foreground", className)}
      title={edge}
    >
      <span className='truncate'>{driver}</span>
      {target && <span className='truncate'>→ {target}</span>}
    </div>
  );
}
