import { Calendar, ChevronDown, ChevronRight, Clock } from "lucide-react";
import { useMemo, useState } from "react";
import type { TopicData, ViewWithData } from "../types";

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

interface FieldsSelectionPanelProps {
  topicData: TopicData | null;
  viewsWithData: ViewWithData[];
  isLoading: boolean;
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (name: string) => void;
  toggleMeasure: (name: string) => void;
  timeDimensions: Array<{ dimension: string; granularity?: string }>;
  onAddTimeDimension: (initialValues?: { dimension: string; granularity: string }) => void;
  onUpdateTimeDimension: (index: number, updates: { granularity?: string }) => void;
  onRemoveTimeDimension: (index: number) => void;
}

const FieldsSelectionPanel = ({
  topicData,
  viewsWithData,
  isLoading,
  selectedDimensions,
  selectedMeasures,
  toggleDimension,
  toggleMeasure,
  timeDimensions,
  onAddTimeDimension,
  onUpdateTimeDimension,
  onRemoveTimeDimension
}: FieldsSelectionPanelProps) => {
  // null means "not yet initialized" - will auto-expand all views
  const [expandedViews, setExpandedViews] = useState<Set<string> | null>(null);
  const [expandedTimeDimensions, setExpandedTimeDimensions] = useState<Set<string>>(new Set());

  const viewNames = useMemo(() => viewsWithData.map((v) => v.viewName), [viewsWithData]);

  // Compute effective expanded views - if null (not initialized), expand all
  const effectiveExpandedViews = useMemo(
    () => expandedViews ?? new Set(viewNames),
    [expandedViews, viewNames]
  );

  const toggleViewExpanded = (viewName: string) => {
    setExpandedViews((prev) => {
      // If null (not initialized), start from all views expanded
      const current = prev ?? new Set(viewNames);
      const next = new Set(current);
      if (next.has(viewName)) {
        next.delete(viewName);
      } else {
        next.add(viewName);
      }
      return next;
    });
  };

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
        {topicData && (
          <div className='py-2'>
            {/* Topic Header */}
            <div className='border-b px-3 py-2'>
              <div className='font-semibold text-sm'>{topicData.name}</div>
            </div>

            {/* Loading state */}
            {isLoading && (
              <div className='px-3 py-2 text-muted-foreground text-xs'>Loading views...</div>
            )}

            {/* Views with their dimensions and measures */}
            <div className='mt-2'>
              {viewsWithData.length === 0 && !isLoading && (
                <div className='px-3 py-2 text-muted-foreground text-xs'>
                  No views found. Check that view files exist in the expected locations.
                </div>
              )}
              {viewsWithData.map((view) => {
                const isExpanded = effectiveExpandedViews.has(view.viewName);
                const isBaseView = view.viewName === topicData.base_view;

                return (
                  <div key={view.viewName} className='mb-1'>
                    {/* View Header */}
                    <button
                      onClick={() => toggleViewExpanded(view.viewName)}
                      className='flex w-full items-center gap-1 px-3 py-1.5 font-medium text-sm hover:bg-muted/50'
                    >
                      {isExpanded ? (
                        <ChevronDown className='h-4 w-4' />
                      ) : (
                        <ChevronRight className='h-4 w-4' />
                      )}
                      <span>{view.viewName}</span>
                      {isBaseView && (
                        <span className='ml-1 text-muted-foreground text-xs'>(base)</span>
                      )}
                    </button>

                    {isExpanded && (
                      <div className='ml-4'>
                        {/* Dimensions */}
                        {view.dimensions.length > 0 && (
                          <div className='mt-1'>
                            <div className='px-4 py-1 font-medium text-muted-foreground text-xs'>
                              DIMENSIONS ({view.dimensions.length})
                            </div>
                            {view.dimensions.map((dimension) => {
                              const fullName = `${view.viewName}.${dimension.name}`;
                              const isTime = isTimeDimension(dimension.type);
                              const isExpanded = expandedTimeDimensions.has(fullName);
                              const selectedGranularity = getSelectedGranularity(fullName);
                              const isSelected =
                                selectedDimensions.includes(fullName) || !!selectedGranularity;

                              return (
                                <div key={fullName}>
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
                                          toggleDimension(fullName);
                                        } else {
                                          // For time dimensions, expand to show granularity options
                                          toggleTimeDimensionExpansion(fullName);
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
                                          toggleTimeDimensionExpansion(fullName);
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
                                            return !["hour", "minute", "second"].includes(
                                              option.value
                                            );
                                          }
                                          return true;
                                        })
                                        .map((option) => {
                                          const Icon = option.icon;
                                          const isGranularitySelected =
                                            selectedGranularity === option.value;
                                          return (
                                            <div
                                              key={option.value}
                                              onClick={() =>
                                                handleGranularitySelect(fullName, option.value)
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

                        {/* Measures */}
                        {view.measures.length > 0 && (
                          <div className='mt-1'>
                            <div className='px-4 py-1 font-medium text-muted-foreground text-xs'>
                              MEASURES ({view.measures.length})
                            </div>
                            {view.measures.map((measure) => {
                              const fullName = `${view.viewName}.${measure.name}`;
                              return (
                                <div
                                  key={fullName}
                                  onClick={() => toggleMeasure(fullName)}
                                  className={`flex cursor-pointer items-start gap-2 px-8 py-1.5 ${
                                    selectedMeasures.includes(fullName)
                                      ? "border-l-2 border-l-primary bg-primary/10"
                                      : "hover:bg-muted/50"
                                  }`}
                                >
                                  <div className='min-w-0 flex-1'>
                                    <div className='truncate text-sm'>{measure.name}</div>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default FieldsSelectionPanel;
