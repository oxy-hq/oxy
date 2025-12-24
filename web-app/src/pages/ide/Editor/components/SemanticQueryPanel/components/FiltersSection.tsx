import { SemanticQueryFilter } from "@/services/api/semantic";
import FilterRow from "./FilterRow";

interface FiltersSectionProps {
  filters: SemanticQueryFilter[];
  availableDimensions: { label: string; value: string }[];
  onUpdateFilter: (index: number, updates: SemanticQueryFilter) => void;
  onRemoveFilter: (index: number) => void;
}

const FiltersSection = ({
  filters,
  availableDimensions,
  onUpdateFilter,
  onRemoveFilter,
}: FiltersSectionProps) => {
  if (filters.length === 0) return null;

  return (
    <div className="border-b p-3 space-y-2">
      {filters.map((filter, index) => (
        <FilterRow
          key={index}
          filter={filter}
          availableDimensions={availableDimensions}
          onUpdate={(updates) => onUpdateFilter(index, updates)}
          onRemove={() => onRemoveFilter(index)}
        />
      ))}
    </div>
  );
};

export default FiltersSection;
