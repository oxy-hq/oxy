import { useState, useMemo } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { TopicData, ViewWithData } from "../types";

interface FieldsSelectionPanelProps {
  topicData: TopicData | null;
  viewsWithData: ViewWithData[];
  isLoading: boolean;
  isError: boolean;
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (name: string) => void;
  toggleMeasure: (name: string) => void;
}

const FieldsSelectionPanel = ({
  topicData,
  viewsWithData,
  isLoading,
  isError,
  selectedDimensions,
  selectedMeasures,
  toggleDimension,
  toggleMeasure,
}: FieldsSelectionPanelProps) => {
  // null means "not yet initialized" - will auto-expand all views
  const [expandedViews, setExpandedViews] = useState<Set<string> | null>(null);

  const viewNames = useMemo(
    () => viewsWithData.map((v) => v.viewName),
    [viewsWithData],
  );

  // Compute effective expanded views - if null (not initialized), expand all
  const effectiveExpandedViews = useMemo(
    () => expandedViews ?? new Set(viewNames),
    [expandedViews, viewNames],
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
    <div className="w-72 flex flex-col border-r bg-background">
      <div className="flex-1 overflow-auto customScrollbar">
        {topicData && (
          <div className="py-2">
            {/* Topic Header */}
            <div className="px-3 py-2 border-b">
              <div className="font-semibold text-sm">{topicData.name}</div>
            </div>

            {/* Loading state */}
            {isLoading && (
              <div className="px-3 py-2 text-xs text-muted-foreground">
                Loading views...
              </div>
            )}

            {/* Error state */}
            {isError && (
              <div className="px-3 py-2 text-xs text-destructive">
                Error loading some views. Check console for details.
              </div>
            )}

            {/* Views with their dimensions and measures */}
            <div className="mt-2">
              {viewsWithData.length === 0 && !isLoading && (
                <div className="px-3 py-2 text-xs text-muted-foreground">
                  No views found. Check that view files exist in the expected
                  locations.
                </div>
              )}
              {viewsWithData.map((view) => {
                const isExpanded = effectiveExpandedViews.has(view.viewName);
                const isBaseView = view.viewName === topicData.base_view;

                return (
                  <div key={view.viewName} className="mb-1">
                    {/* View Header */}
                    <button
                      onClick={() => toggleViewExpanded(view.viewName)}
                      className="w-full flex items-center gap-1 px-3 py-1.5 hover:bg-muted/50 text-sm font-medium"
                    >
                      {isExpanded ? (
                        <ChevronDown className="w-4 h-4" />
                      ) : (
                        <ChevronRight className="w-4 h-4" />
                      )}
                      <span>{view.viewName}</span>
                      {isBaseView && (
                        <span className="text-xs text-muted-foreground ml-1">
                          (base)
                        </span>
                      )}
                    </button>

                    {isExpanded && (
                      <div className="ml-4">
                        {/* Dimensions */}
                        {view.dimensions.length > 0 && (
                          <div className="mt-1">
                            <div className="px-4 py-1 text-xs font-medium text-muted-foreground">
                              DIMENSIONS ({view.dimensions.length})
                            </div>
                            {view.dimensions.map((dimension) => {
                              const fullName = `${view.viewName}.${dimension.name}`;
                              return (
                                <div
                                  key={fullName}
                                  onClick={() => toggleDimension(fullName)}
                                  className={`flex items-start gap-2 px-8 py-1.5 cursor-pointer ${
                                    selectedDimensions.includes(fullName)
                                      ? "bg-primary/10 border-l-2 border-l-primary"
                                      : "hover:bg-muted/50"
                                  }`}
                                >
                                  <div className="flex-1 min-w-0">
                                    <div className="text-sm truncate">
                                      {dimension.name}
                                    </div>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        )}

                        {/* Measures */}
                        {view.measures.length > 0 && (
                          <div className="mt-1">
                            <div className="px-4 py-1 text-xs font-medium text-muted-foreground">
                              MEASURES ({view.measures.length})
                            </div>
                            {view.measures.map((measure) => {
                              const fullName = `${view.viewName}.${measure.name}`;
                              return (
                                <div
                                  key={fullName}
                                  onClick={() => toggleMeasure(fullName)}
                                  className={`flex items-start gap-2 px-8 py-1.5 cursor-pointer ${
                                    selectedMeasures.includes(fullName)
                                      ? "bg-primary/10 border-l-2 border-l-primary"
                                      : "hover:bg-muted/50"
                                  }`}
                                >
                                  <div className="flex-1 min-w-0">
                                    <div className="text-sm truncate">
                                      {measure.name}
                                    </div>
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
