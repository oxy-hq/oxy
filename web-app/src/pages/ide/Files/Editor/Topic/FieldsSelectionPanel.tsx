import { ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import type { TopicData, ViewWithData } from "../types";

interface FieldsSelectionPanelProps {
  topicData: TopicData | null;
  viewsWithData: ViewWithData[];
  isLoading: boolean;
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (name: string) => void;
  toggleMeasure: (name: string) => void;
}

const FieldsSelectionPanel = ({
  topicData,
  viewsWithData,
  isLoading,
  selectedDimensions,
  selectedMeasures,
  toggleDimension,
  toggleMeasure
}: FieldsSelectionPanelProps) => {
  // null means "not yet initialized" - will auto-expand all views
  const [expandedViews, setExpandedViews] = useState<Set<string> | null>(null);

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
                              return (
                                <div
                                  key={fullName}
                                  onClick={() => toggleDimension(fullName)}
                                  className={`flex cursor-pointer items-start gap-2 px-8 py-1.5 ${
                                    selectedDimensions.includes(fullName)
                                      ? "border-l-2 border-l-primary bg-primary/10"
                                      : "hover:bg-muted/50"
                                  }`}
                                >
                                  <div className='min-w-0 flex-1'>
                                    <div className='truncate text-sm'>{dimension.name}</div>
                                  </div>
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
