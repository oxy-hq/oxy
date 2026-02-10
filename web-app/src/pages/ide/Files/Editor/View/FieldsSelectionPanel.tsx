import { Calendar, ChevronDown, ChevronRight, Clock } from "lucide-react";
import { useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { useViewExplorerContext } from "./contexts/ViewExplorerContext";

const granularityOptions = [
  { value: "value", label: "value (raw)", icon: Clock },
  { value: "year", label: "year", icon: Calendar },
  { value: "quarter", label: "quarter", icon: Calendar },
  { value: "month", label: "month", icon: Calendar },
  { value: "week", label: "week", icon: Calendar },
  { value: "day", label: "day", icon: Calendar },
  { value: "hour", label: "hour", icon: Clock },
  { value: "minute", label: "minute", icon: Clock },
  { value: "second", label: "second", icon: Clock }
];

const FieldsSelectionPanel = () => {
  const {
    viewData,
    selectedDimensions,
    selectedMeasures,
    toggleDimension,
    toggleMeasure,
    timeDimensions,
    onAddTimeDimension,
    onUpdateTimeDimension,
    onRemoveTimeDimension
  } = useViewExplorerContext();

  const [dimensionsExpanded, setDimensionsExpanded] = useState(true);
  const [measuresExpanded, setMeasuresExpanded] = useState(true);
  const [expandedTimeDimensions, setExpandedTimeDimensions] = useState<Set<string>>(new Set());

  if (!viewData) return null;

  const dimensions = viewData.dimensions.map((dimension) => {
    return {
      name: dimension.name,
      fullName: `${viewData.name}.${dimension.name}`,
      type: dimension.type
    };
  });

  const measures = viewData.measures.map((measure) => {
    return {
      name: measure.name,
      fullName: `${viewData.name}.${measure.name}`
    };
  });

  const toggleTimeDimensionExpansion = (dimensionFullName: string) => {
    const newExpanded = new Set(expandedTimeDimensions);
    if (newExpanded.has(dimensionFullName)) {
      newExpanded.delete(dimensionFullName);
    } else {
      newExpanded.add(dimensionFullName);
    }
    setExpandedTimeDimensions(newExpanded);
  };

  const handleGranularitySelect = (dimensionFullName: string, granularity: string) => {
    // Find if this time dimension already exists
    const existingIndex = timeDimensions.findIndex((td) => td.dimension === dimensionFullName);

    if (existingIndex >= 0) {
      const currentGranularity = timeDimensions[existingIndex].granularity;
      // If clicking the same granularity, deselect (remove) the time dimension
      if (currentGranularity === granularity) {
        onRemoveTimeDimension(existingIndex);
      } else {
        // Update existing time dimension granularity
        onUpdateTimeDimension(existingIndex, { granularity });
      }
    } else {
      // Add new time dimension with dimension and granularity already set
      onAddTimeDimension({
        dimension: dimensionFullName,
        granularity
      });
    }
  };

  const isTimeDimension = (type: string) => {
    return type === "date" || type === "datetime";
  };

  const getSelectedGranularity = (dimensionFullName: string): string | undefined => {
    const td = timeDimensions.find((td) => td.dimension === dimensionFullName);
    return td?.granularity;
  };

  return (
    <div className='flex w-72 flex-col border-r bg-background'>
      <div className='customScrollbar flex-1 overflow-auto'>
        <div className='py-2'>
          {/* View Header */}
          <div className='border-b px-3 py-2'>
            <div className='font-semibold text-sm'>{viewData.name}</div>
          </div>

          {/* Metadata */}
          <div className='space-y-1 border-b px-3 py-2 text-xs'>
            <div className='flex justify-between gap-2'>
              <span className='shrink-0 text-muted-foreground'>Data Source:</span>
              <span className='truncate font-mono'>{viewData.datasource}</span>
            </div>
            <div className='flex justify-between gap-2'>
              <span className='shrink-0 text-muted-foreground'>Table:</span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className='cursor-help truncate font-mono'>{viewData.table}</span>
                </TooltipTrigger>
                <TooltipContent>{viewData.table}</TooltipContent>
              </Tooltip>
            </div>
          </div>

          {/* Dimensions Section */}
          <div className='mt-2'>
            <button
              onClick={() => setDimensionsExpanded(!dimensionsExpanded)}
              className='flex w-full items-center gap-1 px-3 py-1.5 font-medium text-sm hover:bg-muted/50'
            >
              {dimensionsExpanded ? (
                <ChevronDown className='h-4 w-4' />
              ) : (
                <ChevronRight className='h-4 w-4' />
              )}
              <span>Dimensions</span>
              <span className='ml-auto text-muted-foreground text-xs'>
                {viewData.dimensions.length}
              </span>
            </button>
            {dimensionsExpanded && (
              <div className='py-1'>
                {dimensions.map((dimension) => {
                  const isTime = isTimeDimension(dimension.type);
                  const isExpanded = expandedTimeDimensions.has(dimension.fullName);
                  const selectedGranularity = getSelectedGranularity(dimension.fullName);
                  const isSelected =
                    selectedDimensions.includes(dimension.fullName) || !!selectedGranularity;

                  return (
                    <div key={dimension.name}>
                      {/* Dimension Row */}
                      <div
                        className={`flex cursor-pointer items-start gap-2 px-8 py-1.5 ${
                          isSelected
                            ? "border-l-2 border-l-primary bg-primary/10"
                            : "hover:bg-muted/50"
                        }`}
                      >
                        <div
                          onClick={() => {
                            if (!isTime) {
                              toggleDimension(dimension.fullName);
                            } else {
                              // For time dimensions, expand to show granularity options
                              toggleTimeDimensionExpansion(dimension.fullName);
                            }
                          }}
                          className='min-w-0 flex-1'
                        >
                          <div className='flex items-center gap-1.5'>
                            <div className='truncate text-sm'>{dimension.name}</div>
                            {selectedGranularity && (
                              <span className='text-muted-foreground text-xs'>
                                ({selectedGranularity})
                              </span>
                            )}
                          </div>
                        </div>

                        {isTime && <Clock className='h-3.5 w-3.5 text-blue-500' />}
                        {isTime && (
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              toggleTimeDimensionExpansion(dimension.fullName);
                            }}
                            className='mt-0.5 flex-shrink-0'
                          >
                            {isExpanded ? (
                              <ChevronDown className='h-3 w-3' />
                            ) : (
                              <ChevronRight className='h-3 w-3' />
                            )}
                          </button>
                        )}
                      </div>

                      {/* Granularity Options (for time dimensions) */}
                      {isTime && isExpanded && (
                        <div className='bg-muted/30 py-1'>
                          {granularityOptions
                            .filter((option) => {
                              // For date type, exclude hour, minute, second
                              if (dimension.type === "date") {
                                return !["hour", "minute", "second"].includes(option.value);
                              }
                              return true;
                            })
                            .map((option) => {
                              const Icon = option.icon;
                              const isGranularitySelected = selectedGranularity === option.value;
                              return (
                                <div
                                  key={option.value}
                                  onClick={() =>
                                    handleGranularitySelect(dimension.fullName, option.value)
                                  }
                                  className={`flex cursor-pointer items-center gap-2 py-1.5 pr-8 pl-16 text-sm ${
                                    isGranularitySelected
                                      ? "bg-primary/20 font-medium"
                                      : "hover:bg-muted/50"
                                  }`}
                                >
                                  <Icon className='h-3.5 w-3.5' />
                                  <span>{option.label}</span>
                                </div>
                              );
                            })}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Measures Section */}
          <div className='mt-1'>
            <button
              onClick={() => setMeasuresExpanded(!measuresExpanded)}
              className='flex w-full items-center gap-1 px-3 py-1.5 font-medium text-sm hover:bg-muted/50'
            >
              {measuresExpanded ? (
                <ChevronDown className='h-4 w-4' />
              ) : (
                <ChevronRight className='h-4 w-4' />
              )}
              <span>Measures</span>
              <span className='ml-auto text-muted-foreground text-xs'>
                {viewData.measures.length}
              </span>
            </button>
            {measuresExpanded && (
              <div className='py-1'>
                {measures.map((measure) => (
                  <div
                    key={measure.name}
                    onClick={() => toggleMeasure(measure.fullName)}
                    className={`flex cursor-pointer items-start gap-2 px-8 py-1.5 ${
                      selectedMeasures.includes(measure.fullName)
                        ? "border-l-2 border-l-primary bg-primary/10"
                        : "hover:bg-muted/50"
                    }`}
                  >
                    <div className='min-w-0 flex-1'>
                      <div className='truncate text-sm'>{measure.name}</div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default FieldsSelectionPanel;
