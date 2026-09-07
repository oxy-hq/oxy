import type { MetricEdge, MetricNode, MetricTree } from "@/types/metricTree";
import { MetaBadge, Row, SectionHeader } from "../../components/semanticGraph";
import { measureDescription, measureTitle } from "../measureTitle";
import { ROLE_MARKS } from "./nodeRoles";

interface DefinitionPanelProps {
  measureId: string | null;
  tree: MetricTree;
}

export function DefinitionPanel({ measureId, tree }: DefinitionPanelProps) {
  if (!measureId) {
    return (
      <p className='p-4 text-muted-foreground text-xs'>
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
        <Section title='Components' count={components.length}>
          {components.map((e) => (
            <EdgeRow key={e.from} edge={e} label={e.from} tree={tree} />
          ))}
        </Section>
      )}
      {drivers.length > 0 && (
        <Section title='Declared drivers' count={drivers.length}>
          {drivers.map((e) => (
            <EdgeRow key={e.from} edge={e} label={e.from} tree={tree} />
          ))}
        </Section>
      )}
      {drives.length > 0 && (
        <Section title='Drives' count={drives.length}>
          {drives.map((e) => (
            <EdgeRow key={e.to} edge={e} label={e.to} tree={tree} />
          ))}
        </Section>
      )}
    </div>
  );
}

function NodeMeta({ node }: { node: MetricNode }) {
  const mark = node.is_composite ? ROLE_MARKS.composite : null;
  const description = measureDescription(node);

  return (
    <div className='flex flex-col gap-2'>
      <div className='flex flex-wrap items-center gap-1.5'>
        <span className='font-medium text-[12px] text-foreground'>{measureTitle(node)}</span>
        <MetaBadge>{node.measure_type}</MetaBadge>
        {mark && (
          <MetaBadge>
            <span className={mark.className}>{mark.symbol}</span> {mark.label}
          </MetaBadge>
        )}
      </div>
      {description && (
        <p className='text-[11px] text-muted-foreground leading-relaxed'>{description}</p>
      )}
      <div className='flex flex-col gap-0.5 font-mono text-[10px]'>
        <MetaRow label='view' value={node.view} />
        <MetaRow label='measure' value={node.measure} />
        {node.expr && <MetaRow label='expr' value={node.expr} />}
      </div>
    </div>
  );
}

function Section({
  title,
  count,
  children
}: {
  title: string;
  count: number;
  children: React.ReactNode;
}) {
  return (
    <div className='flex flex-col gap-1.5'>
      <SectionHeader title={title} subtitle={`${count}`} />
      <ul className='flex flex-col gap-1'>{children}</ul>
    </div>
  );
}

function EdgeRow({ edge, label, tree }: { edge: MetricEdge; label: string; tree: MetricTree }) {
  const peer = tree.nodes.find((n) => n.id === label);
  const displayLabel = peer ? measureTitle(peer) : label;
  return (
    <li>
      <Row className='flex-col items-stretch gap-1'>
        <div className='flex min-w-0 items-center justify-between gap-2'>
          <span className='min-w-0 truncate text-foreground' title={displayLabel}>
            {displayLabel}
          </span>
          <DirectionMark direction={edge.direction} />
        </div>
        <div className='flex flex-wrap gap-x-2 gap-y-0.5 text-[9.5px] text-muted-foreground'>
          <span>strength {edge.strength}</span>
          <span>confidence {edge.confidence}</span>
          {edge.coefficient != null && <span>coef {edge.coefficient.toFixed(3)}</span>}
          {edge.form && <span>form {edge.form}</span>}
          {edge.lag != null && <span>lag {edge.lag}d</span>}
          {edge.sign != null && edge.sign !== 1 && <span>sign {edge.sign}</span>}
        </div>
        {edge.description && (
          <p className='text-[9.5px] text-muted-foreground italic'>{edge.description}</p>
        )}
        {edge.refs && edge.refs.length > 0 && (
          <p className='truncate text-[9.5px] text-muted-foreground'>refs {edge.refs.join(", ")}</p>
        )}
      </Row>
    </li>
  );
}

function DirectionMark({ direction }: { direction: MetricEdge["direction"] }) {
  if (direction === "positive")
    return <span className='shrink-0 text-[10px] text-success'>↑ positive</span>;
  if (direction === "negative")
    return <span className='shrink-0 text-[10px] text-destructive'>↓ negative</span>;
  return <span className='shrink-0 text-[10px] text-muted-foreground'>· unknown</span>;
}

function MetaRow({ label, value }: { label: string; value: string }) {
  return (
    <div className='flex gap-1.5'>
      <span className='w-14 shrink-0 text-muted-foreground uppercase tracking-wider'>{label}</span>
      <span className='min-w-0 break-all text-foreground'>{value}</span>
    </div>
  );
}
