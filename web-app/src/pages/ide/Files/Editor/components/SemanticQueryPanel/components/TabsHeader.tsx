import { ArrowUpDown, Download, Filter, Plus, Variable } from "lucide-react";
import Papa from "papaparse";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { handleDownloadFile } from "@/libs/utils/string";
import HeaderActions from "../HeaderActions";

const COLLAPSE_THRESHOLD = 500;

interface TabsHeaderProps {
  showSql: boolean;
  hasResults: boolean;
  result: string[][];
  onAddFilter: () => void;
  onAddOrder: () => void;
  onAddVariable: () => void;
  onExecuteQuery: () => void;
  loading: boolean;
  canExecuteQuery: boolean;
  disabledMessage?: string;
  hasSelectedFields: boolean;
}

const TabsHeader = ({
  showSql,
  hasResults,
  result,
  onAddFilter,
  onAddOrder,
  onAddVariable,
  onExecuteQuery,
  loading,
  canExecuteQuery,
  disabledMessage,
  hasSelectedFields
}: TabsHeaderProps) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isCollapsed, setIsCollapsed] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setIsCollapsed(entry.contentRect.width < COLLAPSE_THRESHOLD);
      }
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const handleDownloadCsv = () => {
    const csvContent = Papa.unparse(result, {
      delimiter: ",",
      header: true,
      skipEmptyLines: true
    });
    const blob = new Blob([csvContent], {
      type: "text/csv;charset=utf-8;"
    });
    handleDownloadFile(blob, "query_results.csv");
  };

  return (
    <div
      ref={containerRef}
      className='scrollbar-none customScrollbar flex min-h-12.5 items-center justify-between gap-4 overflow-x-auto border-b px-4 py-2'
    >
      <TabsList>
        <TabsTrigger value='results'>Results</TabsTrigger>
        <TabsTrigger value='sql'>SQL</TabsTrigger>
      </TabsList>
      <div className='flex flex-shrink-0 items-center gap-2'>
        {!showSql && hasResults && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button size='sm' variant='ghost' onClick={handleDownloadCsv}>
                <Download />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Download results as CSV</TooltipContent>
          </Tooltip>
        )}
        {isCollapsed ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size='sm' variant='outline'>
                <Plus />
                Add
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align='end'>
              <DropdownMenuItem className='cursor-pointer' onClick={onAddFilter}>
                <Filter />
                Add Filter
              </DropdownMenuItem>
              <DropdownMenuItem
                className='cursor-pointer'
                onClick={onAddOrder}
                disabled={!hasSelectedFields}
              >
                <ArrowUpDown />
                Add Sort
              </DropdownMenuItem>
              <DropdownMenuItem className='cursor-pointer' onClick={onAddVariable}>
                <Variable />
                Add Variable
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : (
          <>
            <Button size='sm' variant='outline' onClick={onAddFilter}>
              <Plus />
              Add Filter
            </Button>
            <Button size='sm' variant='outline' onClick={onAddOrder} disabled={!hasSelectedFields}>
              <Plus />
              Add Sort
            </Button>
            <Button size='sm' variant='outline' onClick={onAddVariable}>
              <Plus />
              Add Variable
            </Button>
          </>
        )}
        <div className='flex-shrink-0'>
          <HeaderActions
            onExecuteQuery={onExecuteQuery}
            loading={loading}
            disabled={!canExecuteQuery}
            disabledMessage={disabledMessage}
          />
        </div>
      </div>
    </div>
  );
};

export default TabsHeader;
