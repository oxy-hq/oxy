import type { SemanticQueryPayload } from "@/services/api/analytics";

type Props = {
  query: SemanticQueryPayload;
  database?: string;
};

const formatFilter = (f: NonNullable<SemanticQueryPayload["filters"]>[number]) => {
  const values = f.values?.length ? ` [${f.values.join(", ")}]` : "";
  return `${f.member} ${f.operator}${values}`;
};

const formatTimeDim = (t: NonNullable<SemanticQueryPayload["time_dimensions"]>[number]) => {
  const gran = t.granularity ? ` · ${t.granularity}` : "";
  const range = t.date_range?.length ? ` (${t.date_range.join(" → ")})` : "";
  return `${t.dimension}${gran}${range}`;
};

const Section = ({ label, items }: { label: string; items: string[] }) => {
  if (!items.length) return null;
  return (
    <div>
      <p className='mb-1.5 font-medium text-muted-foreground text-xs'>{label}</p>
      <div className='flex flex-wrap gap-1.5'>
        {items.map((item) => (
          <span key={item} className='rounded-full bg-muted px-2.5 py-0.5 font-mono text-xs'>
            {item}
          </span>
        ))}
      </div>
    </div>
  );
};

export const VerifiedSemanticQueryView = ({ query, database }: Props) => {
  const filters = (query.filters ?? []).map(formatFilter);
  const timeDims = (query.time_dimensions ?? []).map(formatTimeDim);

  const hasMeta = !!database || query.limit != null;

  return (
    <div className='p-4'>
      <div className='space-y-4'>
        {hasMeta && (
          <div className='grid grid-cols-2 gap-2'>
            {database && (
              <div className='rounded border bg-muted/30 px-2.5 py-2'>
                <p className='text-muted-foreground text-xs uppercase tracking-wide'>Database</p>
                <p className='font-medium font-mono text-xs'>{database}</p>
              </div>
            )}
            {query.limit != null && (
              <div className='rounded border bg-muted/30 px-2.5 py-2'>
                <p className='text-muted-foreground text-xs uppercase tracking-wide'>Limit</p>
                <p className='font-medium font-mono text-xs'>{query.limit.toLocaleString()}</p>
              </div>
            )}
          </div>
        )}

        <Section label='Measures' items={query.measures ?? []} />
        <Section label='Dimensions' items={query.dimensions ?? []} />
        <Section label='Time Dimensions' items={timeDims} />
        <Section label='Filters' items={filters} />
        <Section
          label='Order'
          items={(query.order ?? []).map((o) => `${o.id}${o.desc ? " DESC" : ""}`)}
        />
      </div>
    </div>
  );
};
