import FilterRow from "./FilterRow";
import { Filter } from "../../../types";

interface FiltersSectionProps {
  filters: Filter[];
  availableDimensions: { name: string; fullName: string }[];
  onUpdateFilter: (index: number, updates: Filter) => void;
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
      <div>Filters</div>
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
