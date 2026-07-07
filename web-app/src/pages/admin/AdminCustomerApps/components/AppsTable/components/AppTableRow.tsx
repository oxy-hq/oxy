import { Badge } from "@/components/ui/shadcn/badge";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../../../resolveBundleUrl";
import { AppFavicon } from "../../AppFavicon";
import { formatRelativeTime } from "../useAppsTable";
import { AppActionsMenu, StatusPill } from "./AppActionsMenu";
import { CopyButton, OpenAppButton } from "./UrlActions";

interface AppTableRowProps {
  app: CustomerApp;
  showOrg: boolean;
  isSelected: boolean;
  onToggle: (shiftKey: boolean) => void;
  onOpen: (app: CustomerApp) => void;
  onPublish: (app: CustomerApp) => void;
  onUnpublish: (app: CustomerApp) => void;
}

/**
 * One app as a single scannable row: checkbox · name · [org] · source ·
 * status · workspace · last-active · actions. The actions cell carries the
 * visible copy-URL + open-in-new-tab quick buttons (restored from the old
 * list) plus the ⋯ menu. Row click opens the detail; the checkbox and the
 * actions cell stop propagation so acting on a row never also opens it.
 */
export const AppTableRow = ({
  app,
  showOrg,
  isSelected,
  onToggle,
  onOpen,
  onPublish,
  onUnpublish
}: AppTableRowProps) => (
  <TableRow
    data-state={isSelected ? "selected" : undefined}
    className='cursor-pointer'
    onClick={() => onOpen(app)}
  >
    <TableCell className='w-10' onClick={(e) => e.stopPropagation()}>
      <Checkbox
        checked={isSelected}
        onClick={(e) => {
          e.stopPropagation();
          onToggle(e.shiftKey);
        }}
        aria-label={`Select ${app.name}`}
      />
    </TableCell>

    <TableCell>
      <div className='flex items-center gap-2'>
        <AppFavicon app={app} />
        <button
          type='button'
          className='max-w-[26ch] truncate text-left font-medium text-foreground outline-none hover:underline focus-visible:underline'
          onClick={(e) => {
            e.stopPropagation();
            onOpen(app);
          }}
        >
          {app.name}
        </button>
      </div>
    </TableCell>

    {showOrg && (
      <TableCell className='max-w-[140px] truncate font-mono text-muted-foreground text-xs'>
        {app.org_slug}
      </TableCell>
    )}

    <TableCell>
      <Badge variant='outline' className='px-1.5 py-0 font-mono text-[10px] tracking-wide'>
        {app.source_type.toUpperCase()}
      </Badge>
    </TableCell>

    <TableCell>
      <StatusPill isLive={!!app.published_at} />
    </TableCell>

    <TableCell
      className='font-mono text-muted-foreground text-xs'
      title={`Workspace ${app.project_id}`}
    >
      {app.project_id.slice(0, 8)}
    </TableCell>

    <TableCell className='text-muted-foreground text-xs tabular-nums'>
      {formatRelativeTime(app.last_active_at ?? app.last_synced_at)}
    </TableCell>

    <TableCell className='w-28' onClick={(e) => e.stopPropagation()}>
      <div className='flex items-center justify-end gap-0.5'>
        <CopyButton value={resolveBundleUrl(app.url)} label='app URL' />
        <OpenAppButton url={app.url} />
        <AppActionsMenu app={app} onOpen={onOpen} onPublish={onPublish} onUnpublish={onUnpublish} />
      </div>
    </TableCell>
  </TableRow>
);
