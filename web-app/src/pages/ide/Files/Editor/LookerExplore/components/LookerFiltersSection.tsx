import { useLookerExplorerContext } from "../contexts/LookerExplorerContext";
import LookerFilterRow from "./LookerFilterRow";

const LookerFiltersSection = () => {
  const { lookerFilters, allFields, onUpdateLookerFilter, onRemoveLookerFilter } =
    useLookerExplorerContext();

  if (lookerFilters.length === 0) return null;

  return (
    <div className='space-y-2 border-b p-3'>
      <div>Filters</div>
      {lookerFilters.map((filter, index) => (
        <LookerFilterRow
          key={filter.id}
          filter={filter}
          availableFields={allFields}
          onUpdate={(updates) => onUpdateLookerFilter(index, updates)}
          onRemove={() => onRemoveLookerFilter(index)}
        />
      ))}
    </div>
  );
};

export default LookerFiltersSection;
