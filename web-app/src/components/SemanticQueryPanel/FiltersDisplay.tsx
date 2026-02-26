import CollapsibleSection from "./CollapsibleSection";

interface Filter {
  field: string;
  op: string;
  value?: string | number | boolean | string[] | number[];
  [key: string]: unknown;
}

const formatFilterValue = (f: Filter): string => {
  if (f.relative) return String(f.relative);
  if (f.from != null || f.to != null) return `${f.from ?? "…"} → ${f.to ?? "…"}`;
  if (Array.isArray(f.values)) return (f.values as unknown[]).join(", ");
  if (f.value != null) return Array.isArray(f.value) ? f.value.join(", ") : String(f.value);
  return "";
};

interface FiltersDisplayProps {
  filters: Filter[];
}

const FiltersDisplay = ({ filters }: FiltersDisplayProps) => {
  if (filters.length === 0) return null;

  return (
    <CollapsibleSection title='Filters' count={filters.length}>
      <div className='flex flex-wrap gap-1.5'>
        {filters.map((f, i) => (
          <span
            key={`filter-${f.field}-${f.op}-${i}`}
            className='inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs'
          >
            <span className='font-medium'>{f.field.split(".").pop()}</span>
            <span className='text-muted-foreground'>{f.op}</span>
            <span>{formatFilterValue(f)}</span>
          </span>
        ))}
      </div>
    </CollapsibleSection>
  );
};

export default FiltersDisplay;
