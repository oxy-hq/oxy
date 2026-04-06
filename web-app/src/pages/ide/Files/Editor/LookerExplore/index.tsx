import { ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import ErrorAlert from "@/components/ui/ErrorAlert";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator
} from "@/components/ui/shadcn/breadcrumb";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import CollapsibleFieldSection from "../components/SemanticExplorer/CollapsibleFieldSection";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useSemanticExplorerContext } from "../contexts/SemanticExplorerContext";
import LookerFiltersSection from "./components/LookerFiltersSection";
import LookerLoadingIndicator from "./components/LookerLoadingIndicator";
import { LookerExplorerProvider, useLookerExplorerContext } from "./contexts/LookerExplorerContext";

type GroupedLookerView = {
  viewName: string;
  dimensions: Array<{ fullName: string; name: string }>;
  measures: Array<{ fullName: string; name: string }>;
};

const parseLookerField = (fullName: string): { viewName: string; name: string } => {
  const separatorIndex = fullName.indexOf(".");
  if (separatorIndex < 0) {
    return { viewName: "ungrouped", name: fullName };
  }

  return {
    viewName: fullName.slice(0, separatorIndex),
    name: fullName.slice(separatorIndex + 1)
  };
};

const LookerViewSection = ({
  view,
  isExpanded,
  onToggleView,
  selectedDimensions,
  selectedMeasures,
  onToggleDimension,
  onToggleMeasure
}: {
  view: GroupedLookerView;
  isExpanded: boolean;
  onToggleView: (viewName: string) => void;
  selectedDimensions: string[];
  selectedMeasures: string[];
  onToggleDimension: (fieldName: string) => void;
  onToggleMeasure: (fieldName: string) => void;
}) => {
  const hasDimensions = view.dimensions.length > 0;
  const hasMeasures = view.measures.length > 0;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={() => onToggleView(view.viewName)}>
        {isExpanded ? <ChevronDown className='h-4 w-4' /> : <ChevronRight className='h-4 w-4' />}
        <span>{view.viewName}</span>
      </SidebarMenuButton>

      {isExpanded && (
        <SidebarMenuSub className='ml-[15px]'>
          {hasDimensions && (
            <CollapsibleFieldSection title='Dimensions' count={view.dimensions.length}>
              {view.dimensions.map((dimension) => (
                <SidebarMenuSubItem key={dimension.fullName}>
                  <SidebarMenuSubButton
                    onClick={() => onToggleDimension(dimension.fullName)}
                    isActive={selectedDimensions.includes(dimension.fullName)}
                  >
                    <span>{dimension.name}</span>
                  </SidebarMenuSubButton>
                </SidebarMenuSubItem>
              ))}
            </CollapsibleFieldSection>
          )}

          {hasMeasures && (
            <CollapsibleFieldSection title='Measures' count={view.measures.length}>
              {view.measures.map((measure) => (
                <SidebarMenuSubItem key={measure.fullName}>
                  <SidebarMenuSubButton
                    onClick={() => onToggleMeasure(measure.fullName)}
                    isActive={selectedMeasures.includes(measure.fullName)}
                  >
                    <span>{measure.name}</span>
                  </SidebarMenuSubButton>
                </SidebarMenuSubItem>
              ))}
            </CollapsibleFieldSection>
          )}

          {!hasDimensions && !hasMeasures && (
            <p className='px-3 py-2 text-muted-foreground text-xs'>No fields in this view.</p>
          )}
        </SidebarMenuSub>
      )}
    </SidebarMenuItem>
  );
};

const LookerFieldsPanel = () => {
  const {
    dimensions,
    measures,
    exploreName,
    integrationName,
    model,
    exploreLoading,
    exploreError
  } = useLookerExplorerContext();
  const { selectedDimensions, selectedMeasures, toggleDimension, toggleMeasure } =
    useSemanticExplorerContext();

  const groupedViews = useMemo<GroupedLookerView[]>(() => {
    const groupedMap = new Map<string, GroupedLookerView>();

    for (const fullName of dimensions) {
      const parsed = parseLookerField(fullName);
      const existing = groupedMap.get(parsed.viewName);
      if (existing) {
        existing.dimensions.push({ fullName, name: parsed.name });
      } else {
        groupedMap.set(parsed.viewName, {
          viewName: parsed.viewName,
          dimensions: [{ fullName, name: parsed.name }],
          measures: []
        });
      }
    }

    for (const fullName of measures) {
      const parsed = parseLookerField(fullName);
      const existing = groupedMap.get(parsed.viewName);
      if (existing) {
        existing.measures.push({ fullName, name: parsed.name });
      } else {
        groupedMap.set(parsed.viewName, {
          viewName: parsed.viewName,
          dimensions: [],
          measures: [{ fullName, name: parsed.name }]
        });
      }
    }

    return [...groupedMap.values()].sort((a, b) => a.viewName.localeCompare(b.viewName));
  }, [dimensions, measures]);

  const [expandedViews, setExpandedViews] = useState<Set<string> | null>(null);
  const viewNames = useMemo(() => groupedViews.map((view) => view.viewName), [groupedViews]);

  const effectiveExpandedViews = useMemo(
    () => expandedViews ?? new Set(viewNames),
    [expandedViews, viewNames]
  );

  const toggleViewExpanded = (viewName: string) => {
    setExpandedViews((prev) => {
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
  if (exploreError) {
    return (
      <div className='flex w-72 flex-col overflow-hidden border-r bg-sidebar-background p-4'>
        <ErrorAlert message={`Failed to load explore: ${exploreError.message}`} />
      </div>
    );
  }

  if (exploreLoading) {
    return (
      <div className='flex w-72 flex-col overflow-hidden border-r bg-sidebar-background p-4'>
        <div className='text-muted-foreground text-sm'>Loading fields...</div>
      </div>
    );
  }

  return (
    <div className='flex w-72 flex-col overflow-hidden border-r bg-sidebar-background'>
      <SidebarGroupLabel className='flex h-auto min-h-12.5 items-center justify-between rounded-none border-sidebar-border border-b px-2 py-1'>
        <span className='font-semibold text-sm'>{exploreName}</span>
      </SidebarGroupLabel>

      <div className='space-y-1.5 border-sidebar-border border-b px-3 py-2.5 text-sm'>
        <div className='flex items-center justify-between gap-2'>
          <span className='shrink-0 text-muted-foreground'>Integration</span>
          <span className='truncate text-foreground'>{integrationName}</span>
        </div>
        <div className='flex items-center justify-between gap-2'>
          <span className='shrink-0 text-muted-foreground'>Model</span>
          <span className='truncate text-foreground'>{model}</span>
        </div>
      </div>
      <SidebarContent className='h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='px-1 pt-2'>
          {groupedViews.length === 0 && (
            <p className='px-3 py-2 text-muted-foreground text-xs'>
              No fields found. Run <code>oxy looker sync</code> to fetch metadata.
            </p>
          )}
          <SidebarMenu>
            {groupedViews.map((view) => (
              <LookerViewSection
                key={view.viewName}
                view={view}
                isExpanded={effectiveExpandedViews.has(view.viewName)}
                onToggleView={toggleViewExpanded}
                selectedDimensions={selectedDimensions}
                selectedMeasures={selectedMeasures}
                onToggleDimension={toggleDimension}
                onToggleMeasure={toggleMeasure}
              />
            ))}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};

const LookerExplorer = () => {
  const { exploreLoading, exploreError } = useLookerExplorerContext();

  if (exploreError) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center p-4'>
        <ErrorAlert
          title='Error loading explore'
          message={exploreError.message}
          className='max-w-2xl'
        />
      </div>
    );
  }

  if (exploreLoading) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center'>
        <div className='text-muted-foreground'>Loading explore...</div>
      </div>
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex min-h-0 flex-1 gap-4'>
        <LookerFieldsPanel />
        <div className='flex min-h-0 flex-1 flex-col overflow-hidden'>
          <SemanticQueryPanel
            extraSectionAboveSorts={<LookerFiltersSection />}
            showAddVariable={false}
            sqlLoadingIndicator={<LookerLoadingIndicator message='Generating SQL via Looker...' />}
            executeLoadingIndicator={
              <LookerLoadingIndicator message='Executing query on Looker...' />
            }
          />
        </div>
      </div>
    </div>
  );
};

const LookerExplorerPage = () => {
  const { integrationName, model, exploreName } = useParams<{
    integrationName: string;
    model: string;
    exploreName: string;
  }>();

  if (!integrationName || !model || !exploreName) return null;

  const decodedIntegration = decodeURIComponent(integrationName);
  const decodedModel = decodeURIComponent(model);
  const decodedExplore = decodeURIComponent(exploreName);

  return (
    <div className='flex h-full flex-1 flex-col'>
      <div className='flex min-h-10 items-center border-border border-b bg-editor-background px-2 py-1'>
        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem>
              <BreadcrumbPage className='text-muted-foreground'>
                {decodedIntegration}
              </BreadcrumbPage>
            </BreadcrumbItem>
            <BreadcrumbSeparator />
            <BreadcrumbItem>
              <BreadcrumbPage className='text-foreground'>{decodedExplore}</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
      </div>
      <LookerExplorerProvider
        integrationName={decodedIntegration}
        model={decodedModel}
        exploreName={decodedExplore}
      >
        <LookerExplorer />
      </LookerExplorerProvider>
    </div>
  );
};

export default LookerExplorerPage;
