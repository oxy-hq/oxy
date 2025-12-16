import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { ViewData } from "../types";

interface FieldsSelectionPanelProps {
  viewData: ViewData | null;
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (name: string) => void;
  toggleMeasure: (name: string) => void;
}

const FieldsSelectionPanel = ({
  viewData,
  selectedDimensions,
  selectedMeasures,
  toggleDimension,
  toggleMeasure,
}: FieldsSelectionPanelProps) => {
  const [dimensionsExpanded, setDimensionsExpanded] = useState(true);
  const [measuresExpanded, setMeasuresExpanded] = useState(true);

  return (
    <div className="w-72 flex flex-col border-r bg-background">
      <div className="flex-1 overflow-auto customScrollbar">
        {viewData ? (
          <div className="py-2">
            {/* View Header */}
            <div className="px-3 py-2 border-b">
              <div className="font-semibold text-sm">{viewData.name}</div>
            </div>

            {/* Metadata */}
            <div className="px-3 py-2 space-y-1 text-xs border-b">
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground shrink-0">
                  Data Source:
                </span>
                <span className="font-mono truncate">
                  {viewData.datasource}
                </span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground shrink-0">Table:</span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="font-mono truncate cursor-help">
                      {viewData.table}
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>{viewData.table}</TooltipContent>
                </Tooltip>
              </div>
            </div>

            {/* Dimensions Section */}
            <div className="mt-2">
              <button
                onClick={() => setDimensionsExpanded(!dimensionsExpanded)}
                className="w-full flex items-center gap-1 px-3 py-1.5 hover:bg-muted/50 text-sm font-medium"
              >
                {dimensionsExpanded ? (
                  <ChevronDown className="w-4 h-4" />
                ) : (
                  <ChevronRight className="w-4 h-4" />
                )}
                <span>Dimensions</span>
                <span className="text-xs text-muted-foreground ml-auto">
                  {viewData.dimensions.length}
                </span>
              </button>
              {dimensionsExpanded && (
                <div className="py-1">
                  {viewData.dimensions.map((dimension) => (
                    <div
                      key={dimension.name}
                      onClick={() => toggleDimension(dimension.name)}
                      className={`flex items-start gap-2 px-8 py-1.5 cursor-pointer ${
                        selectedDimensions.includes(dimension.name)
                          ? "bg-primary/10 border-l-2 border-l-primary"
                          : "hover:bg-muted/50"
                      }`}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="text-sm truncate">{dimension.name}</div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Measures Section */}
            <div className="mt-1">
              <button
                onClick={() => setMeasuresExpanded(!measuresExpanded)}
                className="w-full flex items-center gap-1 px-3 py-1.5 hover:bg-muted/50 text-sm font-medium"
              >
                {measuresExpanded ? (
                  <ChevronDown className="w-4 h-4" />
                ) : (
                  <ChevronRight className="w-4 h-4" />
                )}
                <span>Measures</span>
                <span className="text-xs text-muted-foreground ml-auto">
                  {viewData.measures.length}
                </span>
              </button>
              {measuresExpanded && (
                <div className="py-1">
                  {viewData.measures.map((measure) => (
                    <div
                      key={measure.name}
                      onClick={() => toggleMeasure(measure.name)}
                      className={`flex items-start gap-2 px-8 py-1.5 cursor-pointer ${
                        selectedMeasures.includes(measure.name)
                          ? "bg-primary/10 border-l-2 border-l-primary"
                          : "hover:bg-muted/50"
                      }`}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="text-sm truncate">{measure.name}</div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="p-4 text-sm text-muted-foreground">
            Unable to parse view file
          </div>
        )}
      </div>
    </div>
  );
};

export default FieldsSelectionPanel;
