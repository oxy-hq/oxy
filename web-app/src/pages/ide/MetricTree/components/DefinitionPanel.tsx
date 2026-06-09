import { Badge } from "@/components/ui/shadcn/badge";
import type { MetricEdge, MetricNode, MetricTree } from "@/types/metricTree";

interface DefinitionPanelProps {
  measureId: string | null;
  tree: MetricTree;
}

export function DefinitionPanel({ measureId, tree }: DefinitionPanelProps) {
  if (!measureId) {
    return (
      <p className='p-4 text-muted-foreground text-sm'>
        Select a measure in the graph to see its definition.
      </p>
    );
  }

  const node = tree.nodes.find((n) => n.id === measureId);
  if (!node) return null;

  const incoming = tree.edges.filter((e) => e.to === measureId);
  const outgoing = tree.edges.filter((e) => e.from === measureId);
  const components = incoming.filter((e) => e.kind === "component");
  const drivers = incoming.filter((e) => e.kind === "driver");
  const drives = outgoing.filter((e) => e.kind === "driver");

  return (
    <div className='flex flex-col gap-4 p-4'>
      <NodeMeta node={node} />
      {components.length > 0 && (
        <Section title='Components'>
          {components.map((e) => (
            <EdgeRow key={e.from} edge={e} label={e.from} tree={tree} />
          ))}
        </Section>
      )}
      {drivers.length > 0 && (
        <Section title='Declared drivers'>
          {drivers.map((e) => (
            <EdgeRow key={e.from} edge={e} label={e.from} tree={tree} />
          ))}
        </Section>
      )}
      {drives.length > 0 && (
        <Section title='Drives'>
          {drives.map((e) => (
            <EdgeRow key={e.to} edge={e} label={e.to} tree={tree} />
          ))}
        </Section>
      )}
    </div>
  );
}

function NodeMeta({ node }: { node: MetricNode }) {
  return (
    <div className='flex flex-col gap-2'>
      <div className='flex flex-wrap items-center gap-1.5'>
        <span className='font-medium text-foreground text-sm'>{node.label || node.measure}</span>
        <Badge variant='outline' className='text-xs'>
          {node.measure_type}
        </Badge>
        {node.is_composite && (
          <Badge variant='secondary' className='text-xs'>
            composite
          </Badge>
        )}
      </div>
      {node.description && <p className='text-muted-foreground text-xs'>{node.description}</p>}
      <div className='flex flex-col gap-0.5 text-xs'>
        <Row label='View' value={node.view} />
        <Row label='Measure' value={node.measure} mono />
        {node.expr && <Row label='Expression' value={node.expr} mono />}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className='flex flex-col gap-1.5'>
      <p className='text-muted-foreground text-xs uppercase tracking-wide'>{title}</p>
      <ul className='flex flex-col gap-1.5'>{children}</ul>
    </div>
  );
}

function EdgeRow({ edge, label, tree }: { edge: MetricEdge; label: string; tree: MetricTree }) {
  const peer = tree.nodes.find((n) => n.id === label);
  const displayLabel = peer?.label || label;
  return (
    <li className='rounded-md border border-border bg-card p-2.5 text-sm'>
      <div className='flex items-center justify-between gap-2'>
        <span className='font-medium'>{displayLabel}</span>
        <DirectionBadge direction={edge.direction} />
      </div>
      <div className='mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-muted-foreground text-xs'>
        <span>strength: {edge.strength}</span>
        <span>confidence: {edge.confidence}</span>
        {edge.coefficient != null && <span>coef: {edge.coefficient.toFixed(3)}</span>}
        {edge.form && <span>form: {edge.form}</span>}
        {edge.lag != null && <span>lag: {edge.lag}d</span>}
        {edge.sign != null && edge.sign !== 1 && <span>sign: {edge.sign}</span>}
      </div>
      {edge.description && (
        <p className='mt-1 text-muted-foreground text-xs italic'>{edge.description}</p>
      )}
      {edge.refs && edge.refs.length > 0 && (
        <p className='mt-1 text-muted-foreground text-xs'>refs: {edge.refs.join(", ")}</p>
      )}
    </li>
  );
}

function DirectionBadge({ direction }: { direction: MetricEdge["direction"] }) {
  if (direction === "positive")
    return (
      <Badge variant='outline' className='border-success/50 text-success text-xs'>
        ↑ positive
      </Badge>
    );
  if (direction === "negative")
    return (
      <Badge variant='outline' className='border-destructive/50 text-destructive text-xs'>
        ↓ negative
      </Badge>
    );
  return (
    <Badge variant='outline' className='text-xs'>
      unknown
    </Badge>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className='flex gap-1.5'>
      <span className='w-20 shrink-0 text-muted-foreground'>{label}</span>
      <span className={mono ? "t-code break-all text-foreground" : "text-foreground"}>{value}</span>
    </div>
  );
}
