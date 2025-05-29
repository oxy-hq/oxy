import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuTrigger,
} from "@/components/ui/shadcn/dropdown-menu";
import { DropdownMenu } from "@/components/ui/shadcn/dropdown-menu";
import { ChevronDown, Table } from "lucide-react";
import { useMemo } from "react";

type Props = {
  onSelect: (datasets: string[]) => void;
  selectedDatasets: string[];
  availableDatasets: Record<string, string[]>;
  disabled?: boolean;
};

const DatasetDropdown = ({
  onSelect,
  selectedDatasets,
  availableDatasets,
  disabled = false,
}: Props) => {
  const datasetOptions = useMemo(() => {
    const options: string[] = [];
    Object.keys(availableDatasets).forEach((dataset) => {
      if (dataset) {
        // Skip empty dataset names
        options.push(dataset);
      }
    });
    return options.sort();
  }, [availableDatasets]);

  const displayText = useMemo(() => {
    if (selectedDatasets.length === 0) {
      return "All Datasets";
    } else if (selectedDatasets.length === 1) {
      return selectedDatasets[0];
    } else {
      return `${selectedDatasets.length} selected`;
    }
  }, [selectedDatasets]);

  const handleDatasetToggle = (dataset: string) => {
    const newSelection = selectedDatasets.includes(dataset)
      ? selectedDatasets.filter((d) => d !== dataset)
      : [...selectedDatasets, dataset];
    onSelect(newSelection);
  };

  if (datasetOptions.length === 0) {
    return null;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger disabled={disabled}>
        <Button
          disabled={disabled}
          variant="outline"
          className="bg-sidebar-background border-sidebar-background"
        >
          <Table className="h-4 w-4" />
          <span>{displayText}</span>
          <ChevronDown className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="customScrollbar">
        <DropdownMenuCheckboxItem
          checked={selectedDatasets.length === 0}
          onCheckedChange={() => onSelect([])}
        >
          <span className="font-medium">All Datasets</span>
        </DropdownMenuCheckboxItem>
        {datasetOptions.map((dataset) => (
          <DropdownMenuCheckboxItem
            key={dataset}
            checked={selectedDatasets.includes(dataset)}
            onCheckedChange={() => handleDatasetToggle(dataset)}
          >
            <div className="flex flex-col">
              <span>{dataset}</span>
              <span className="text-xs text-muted-foreground">
                {availableDatasets[dataset]?.length || 0} tables
              </span>
            </div>
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default DatasetDropdown;
