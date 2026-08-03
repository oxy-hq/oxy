import { LayoutGrid, List, Plus, Search } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import type { AppsTableState, ViewMode } from "../useAppsTable";

interface AppsToolbarProps {
  state: AppsTableState;
  setState: (patch: Partial<AppsTableState>) => void;
  onCreate: () => void;
  filteredCount: number;
  totalCount: number;
}

/**
 * One-row control bar: search, the view controls (group / status / source),
 * a gallery↔list toggle, and the primary Create action anchored right. Org is
 * no longer its own filter — search matches org slug and "Group by org"
 * restores the historical layout, so the bar stays lean.
 */
export const AppsToolbar = ({
  state,
  setState,
  onCreate,
  filteredCount,
  totalCount
}: AppsToolbarProps) => (
  <div className='flex shrink-0 flex-wrap items-center gap-2 border-b bg-background/60 p-3'>
    <div className='relative min-w-48 flex-1'>
      <Search className='absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground' />
      <Input
        value={state.q}
        onChange={(e) => setState({ q: e.target.value })}
        placeholder='Search name, org, workspace…'
        className='h-7 pl-7'
      />
    </div>

    <Select
      value={state.group}
      onValueChange={(v) => setState({ group: v as AppsTableState["group"] })}
    >
      <SelectTrigger className='h-7 w-auto gap-1.5 px-2 text-xs' aria-label='Group by'>
        <span className='text-muted-foreground'>Group:</span>
        <SelectValue />
      </SelectTrigger>
      <SelectContent align='end'>
        <SelectItem value='org'>Org</SelectItem>
        <SelectItem value='status'>Status</SelectItem>
        <SelectItem value='source'>Source</SelectItem>
        <SelectItem value='none'>None</SelectItem>
      </SelectContent>
    </Select>

    <Select
      value={state.status}
      onValueChange={(v) => setState({ status: v as AppsTableState["status"] })}
    >
      <SelectTrigger className='h-7 w-auto gap-1 px-2 text-xs' aria-label='Filter by status'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent align='end'>
        <SelectItem value='all'>Any status</SelectItem>
        <SelectItem value='live'>Live</SelectItem>
        <SelectItem value='draft'>Draft</SelectItem>
      </SelectContent>
    </Select>

    <Select
      value={state.source}
      onValueChange={(v) => setState({ source: v as AppsTableState["source"] })}
    >
      <SelectTrigger className='h-7 w-auto gap-1 px-2 text-xs' aria-label='Filter by source'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent align='end'>
        <SelectItem value='all'>Any source</SelectItem>
        <SelectItem value='s3'>S3</SelectItem>
        <SelectItem value='local'>Local</SelectItem>
        <SelectItem value='v0'>v0</SelectItem>
      </SelectContent>
    </Select>

    <ToggleGroup
      type='single'
      size='sm'
      variant='outline'
      value={state.view}
      onValueChange={(v) => v && setState({ view: v as ViewMode })}
      className='h-8'
    >
      <ToggleGroupItem value='gallery' aria-label='Gallery view' className='h-8 px-2'>
        <LayoutGrid className='size-3.5' />
      </ToggleGroupItem>
      <ToggleGroupItem value='list' aria-label='List view' className='h-8 px-2'>
        <List className='size-3.5' />
      </ToggleGroupItem>
    </ToggleGroup>

    <span className='hidden text-muted-foreground text-xs tabular-nums sm:inline'>
      {filteredCount === totalCount ? `${totalCount}` : `${filteredCount} / ${totalCount}`}
    </span>

    <Button size='icon' className='size-8 shrink-0' onClick={onCreate} aria-label='Create app'>
      <Plus className='size-3.5' />
    </Button>
  </div>
);
