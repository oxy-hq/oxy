import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { useViewExplorerContext } from "./contexts/ViewExplorerContext";

const FieldsSelectionPanel = () => {
  const { viewData, selectedDimensions, selectedMeasures, toggleDimension, toggleMeasure } =
    useViewExplorerContext();

  const [dimensionsExpanded, setDimensionsExpanded] = useState(true);
  const [measuresExpanded, setMeasuresExpanded] = useState(true);

  if (!viewData) return null;

  const dimensions = viewData.dimensions.map((dimension) => {
    return {
      name: dimension.name,
      fullName: `${viewData.name}.${dimension.name}`
    };
  });

  const measures = viewData.measures.map((measure) => {
    return {
      name: measure.name,
      fullName: `${viewData.name}.${measure.name}`
    };
  });

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
                {dimensions.map((dimension) => (
                  <div
                    key={dimension.name}
                    onClick={() => toggleDimension(dimension.fullName)}
                    className={`flex cursor-pointer items-start gap-2 px-8 py-1.5 ${
                      selectedDimensions.includes(dimension.fullName)
                        ? "border-l-2 border-l-primary bg-primary/10"
                        : "hover:bg-muted/50"
                    }`}
                  >
                    <div className='min-w-0 flex-1'>
                      <div className='truncate text-sm'>{dimension.name}</div>
                    </div>
                  </div>
                ))}
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
