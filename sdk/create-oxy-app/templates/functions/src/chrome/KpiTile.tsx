import { cn } from "../cn";
import { Panel } from "./Panel";

// A single stat: the label becomes the Panel caption, the value is a big mono
// number, and an optional `sub` line carries a caption. `loading` swaps the
// value for a muted placeholder so the KPI row keeps its shape while the query
// resolves.
export function KpiTile({
  label,
  value,
  sub,
  loading
}: {
  label: string;
  value: string;
  sub?: string;
  loading?: boolean;
}) {
  return (
    <Panel title={label}>
      <div
        className={cn(
          "font-mono text-xl leading-none",
          loading ? "text-muted-foreground/40" : "text-foreground"
        )}
      >
        {loading ? "···" : value}
      </div>
      {sub && !loading && <div className='font-mono text-[10px] text-muted-foreground'>{sub}</div>}
    </Panel>
  );
}
