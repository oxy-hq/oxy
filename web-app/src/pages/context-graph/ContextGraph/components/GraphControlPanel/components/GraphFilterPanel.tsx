import { Filter } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { FOCUS_OPTIONS, type FocusType } from "../../../constants";

interface GraphFilterPanelProps {
  focusType: FocusType;
  onFocusTypeChange: (type: FocusType) => void;
  expandAll: boolean;
  onExpandAllChange: (expand: boolean) => void;
  focusedNodeId: string | null;
  onReset: () => void;
}

export function GraphFilterPanel({
  focusType,
  onFocusTypeChange,
  expandAll,
  onExpandAllChange,
  focusedNodeId,
  onReset
}: GraphFilterPanelProps) {
  return (
    <div className='mt-3 border-sidebar-border border-t pt-3'>
      <div className='mb-2 flex items-center gap-2'>
        <Filter className='h-4 w-4 text-sidebar-foreground/70' />
        <span className='font-semibold text-sidebar-foreground text-sm'>Focus View</span>
      </div>
      <Select value={focusType} onValueChange={(value) => onFocusTypeChange(value as FocusType)}>
        <SelectTrigger
          className='h-9 border-sidebar-border bg-sidebar-accent text-sidebar-foreground text-sm'
          data-testid='context-graph-filter-type'
        >
          <SelectValue placeholder='Select focus' />
        </SelectTrigger>
        <SelectContent>
          {FOCUS_OPTIONS.map(({ value, label, icon }) => (
            <SelectItem key={value} value={value} className='cursor-pointer text-sm'>
              <div className='flex items-center gap-2'>
                {icon}
                <span>{label}</span>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div className='mt-2 border-sidebar-border border-t pt-2'>
        <div
          className={`flex items-center gap-2 ${
            focusedNodeId ? "cursor-pointer" : "cursor-not-allowed opacity-50"
          }`}
        >
          <Checkbox
            id='expand-all'
            checked={expandAll}
            onCheckedChange={(checked) => onExpandAllChange(checked === true)}
            disabled={!focusedNodeId}
          />
          <label htmlFor='expand-all' className='text-sm'>
            Expand all connected
          </label>
        </div>
        <p className='mt-1 text-muted-foreground text-xs'>Show entire cluster when clicked</p>
      </div>

      {focusedNodeId && (
        <div className='mt-2 border-sidebar-border border-t pt-2'>
          <Button onClick={onReset} variant='outline' size='sm' className='w-full text-sm'>
            Reset View
          </Button>
        </div>
      )}
    </div>
  );
}
