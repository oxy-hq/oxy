import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu
} from "@/components/ui/shadcn/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import CollapsibleFieldSection from "../components/SemanticExplorer/CollapsibleFieldSection";
import DimensionItem from "../components/SemanticExplorer/DimensionItem";
import MeasureItem from "../components/SemanticExplorer/MeasureItem";
import {
  isTimeDimension,
  useTimeDimensionHandlers
} from "../components/SemanticExplorer/useTimeDimensionHandlers";
import { useViewExplorerContext } from "./contexts/ViewExplorerContext";

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

  const { handleGranularitySelect, getSelectedGranularity } = useTimeDimensionHandlers(
    timeDimensions,
    onAddTimeDimension,
    onUpdateTimeDimension,
    onRemoveTimeDimension
  );

  if (!viewData) return null;

  const dimensions = viewData.dimensions.map((dimension) => ({
    name: dimension.name,
    fullName: `${viewData.name}.${dimension.name}`,
    type: dimension.type
  }));

  const measures = viewData.measures.map((measure) => ({
    name: measure.name,
    fullName: `${viewData.name}.${measure.name}`
  }));

  return (
    <div className='flex w-72 flex-col overflow-hidden border-r bg-sidebar-background'>
      <SidebarGroupLabel className='flex h-auto min-h-12.5 items-center justify-between rounded-none border-sidebar-border border-b px-2 py-1'>
        <span className='font-semibold text-sm'>{viewData.name}</span>
      </SidebarGroupLabel>

      {/* Metadata */}
      <div className='space-y-1.5 border-sidebar-border border-b px-3 py-2.5 text-sm'>
        <div className='flex items-center justify-between gap-2'>
          <span className='shrink-0 text-muted-foreground'>Data source</span>
          <span className='truncate text-foreground'>{viewData.datasource}</span>
        </div>
        <div className='flex items-center justify-between gap-2'>
          <span className='shrink-0 text-muted-foreground'>Table</span>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className='max-w-[140px] cursor-help truncate text-foreground'>
                {viewData.table}
              </span>
            </TooltipTrigger>
            <TooltipContent side='left'>{viewData.table}</TooltipContent>
          </Tooltip>
        </div>
      </div>

      <SidebarContent className='customScrollbar h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='px-1 pt-2'>
          <SidebarMenu>
            <CollapsibleFieldSection title='Dimensions' count={viewData.dimensions.length}>
              {dimensions.map((dimension) => {
                const isTime = isTimeDimension(dimension.type);
                const selectedGranularity = getSelectedGranularity(dimension.fullName);

                return (
                  <DimensionItem
                    key={dimension.name}
                    name={dimension.name}
                    fullName={dimension.fullName}
                    type={dimension.type}
                    isSelected={
                      selectedDimensions.includes(dimension.fullName) || !!selectedGranularity
                    }
                    selectedGranularity={selectedGranularity}
                    isTimeDimension={isTime}
                    onToggle={() => toggleDimension(dimension.fullName)}
                    onGranularitySelect={handleGranularitySelect}
                  />
                );
              })}
            </CollapsibleFieldSection>

            <CollapsibleFieldSection title='Measures' count={viewData.measures.length}>
              {measures.map((measure) => (
                <MeasureItem
                  key={measure.name}
                  name={measure.name}
                  isSelected={selectedMeasures.includes(measure.fullName)}
                  onToggle={() => toggleMeasure(measure.fullName)}
                />
              ))}
            </CollapsibleFieldSection>
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};

export default FieldsSelectionPanel;
