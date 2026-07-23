import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import { cn } from "@/libs/shadcn/utils";
import type { SortDir, SortKey } from "../useAppsTable";

interface AppsTableHeaderProps {
  showOrg: boolean;
  sortKey: SortKey;
  sortDir: SortDir;
  onSort: (key: SortKey) => void;
  allSelected: boolean;
  someSelected: boolean;
  onToggleAll: () => void;
}

/**
 * Sticky column header. Sortable columns are buttons that toggle direction on
 * re-click; the active column shows an up/down arrow, the rest a faint
 * up-down hint on hover. Workspace + Actions are not sortable.
 */
export const AppsTableHeader = ({
  showOrg,
  sortKey,
  sortDir,
  onSort,
  allSelected,
  someSelected,
  onToggleAll
}: AppsTableHeaderProps) => (
  <TableHeader className='sticky top-0 z-10 bg-background'>
    <TableRow className='hover:bg-transparent'>
      <TableHead className='w-10'>
        <Checkbox
          checked={allSelected ? true : someSelected ? "indeterminate" : false}
          onCheckedChange={onToggleAll}
          aria-label='Select all apps'
        />
      </TableHead>
      <SortHead col='name' label='Name' active={sortKey} dir={sortDir} onSort={onSort} />
      {showOrg && <SortHead col='org' label='Org' active={sortKey} dir={sortDir} onSort={onSort} />}
      <SortHead col='source' label='Source' active={sortKey} dir={sortDir} onSort={onSort} />
      <SortHead col='status' label='Status' active={sortKey} dir={sortDir} onSort={onSort} />
      <TableHead className='font-medium text-muted-foreground text-xs'>Workspace</TableHead>
      <SortHead col='active' label='Active' active={sortKey} dir={sortDir} onSort={onSort} />
      <TableHead className='w-10'>
        <span className='sr-only'>Actions</span>
      </TableHead>
    </TableRow>
  </TableHeader>
);

const SortHead = ({
  col,
  label,
  active,
  dir,
  onSort
}: {
  col: SortKey;
  label: string;
  active: SortKey;
  dir: SortDir;
  onSort: (key: SortKey) => void;
}) => {
  const isActive = active === col;
  return (
    <TableHead className='p-0'>
      <button
        type='button'
        onClick={() => onSort(col)}
        className={cn(
          "group flex h-full w-full items-center gap-1 px-2 py-2 text-left font-medium text-xs outline-none focus-visible:underline",
          isActive ? "text-foreground" : "text-muted-foreground hover:text-foreground"
        )}
      >
        {label}
        {isActive ? (
          dir === "asc" ? (
            <ArrowUp className='size-3' />
          ) : (
            <ArrowDown className='size-3' />
          )
        ) : (
          <ChevronsUpDown className='size-3 opacity-0 transition-opacity group-hover:opacity-50' />
        )}
      </button>
    </TableHead>
  );
};
