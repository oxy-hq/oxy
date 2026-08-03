import { AppWindow, Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { usePublishApp, useUnpublishApp } from "@/hooks/api/customApps/useCustomApps";
import { useRowSelection } from "@/hooks/useRowSelection";
import type { CustomApp } from "@/types/apps";
import { AppsGallery, type AppsViewProps } from "./components/AppsGallery";
import { AppsListView } from "./components/AppsListView";
import { AppsToolbar } from "./components/AppsToolbar";
import { BulkActionBar } from "./components/BulkActionBar";
import {
  buildAppsTableModel,
  defaultDirFor,
  type SortKey,
  useAppsTableState
} from "./useAppsTable";

interface AppsTableProps {
  apps: CustomApp[];
  isLoading: boolean;
  /** True while background pages are still streaming in (auto-load-all). */
  isLoadingMore: boolean;
  onSelect: (app: CustomApp) => void;
  onCreate: () => void;
}

/**
 * The custom-app registry browser: a toolbar (search / group / filters /
 * gallery-list toggle), a card **or** list view over the same filtered+grouped
 * model, row selection, and a sticky bulk-action bar. Rich per-app detail
 * opens as a full page via `onSelect`.
 */
export const AppsTable = ({
  apps,
  isLoading,
  isLoadingMore,
  onSelect,
  onCreate
}: AppsTableProps) => {
  const [state, setState] = useAppsTableState();
  const model = useMemo(() => buildAppsTableModel(apps, state), [apps, state]);
  const selection = useRowSelection(model.flatIds);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const publishApp = usePublishApp();
  const unpublishApp = useUnpublishApp();

  const showOrg = state.group !== "org";
  const showGroupHeaders = state.group !== "none";

  const onSort = (key: SortKey) => {
    if (state.sortKey === key) {
      setState({ sortDir: state.sortDir === "asc" ? "desc" : "asc" });
    } else {
      setState({ sortKey: key, sortDir: defaultDirFor(key) });
    }
  };

  const toggleCollapse = (key: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const viewProps: AppsViewProps = {
    groups: model.groups,
    showOrg,
    showGroupHeaders,
    collapsed,
    onToggleCollapse: toggleCollapse,
    isSelected: selection.isSelected,
    onToggleRow: selection.toggle,
    onToggleGroup: selection.setMany,
    onOpen: onSelect,
    onPublish: (a) => publishApp.mutate(a.id),
    onUnpublish: (a) => unpublishApp.mutate(a.id)
  };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <AppsToolbar
        state={state}
        setState={setState}
        onCreate={onCreate}
        filteredCount={model.filteredCount}
        totalCount={model.totalCount}
      />

      {isLoading ? (
        <CenteredState>
          <span className='flex items-center gap-2'>
            <Spinner className='size-4' /> Loading apps…
          </span>
        </CenteredState>
      ) : model.totalCount === 0 ? (
        <CenteredState>
          <EmptyState onCreate={onCreate} />
        </CenteredState>
      ) : model.filteredCount === 0 ? (
        <CenteredState>No apps match these filters.</CenteredState>
      ) : state.view === "gallery" ? (
        <AppsGallery {...viewProps} />
      ) : (
        <AppsListView
          {...viewProps}
          sortKey={state.sortKey}
          sortDir={state.sortDir}
          onSort={onSort}
          allSelected={selection.allSelected}
          someSelected={selection.someSelected}
          onToggleAll={selection.toggleAll}
        />
      )}

      {isLoadingMore && (
        <div className='flex shrink-0 items-center justify-center gap-2 border-t py-1.5 text-muted-foreground text-xs'>
          <Spinner className='size-3' /> Loading all apps…
        </div>
      )}

      <BulkActionBar selectedIds={selection.selectedIds} onClear={selection.clear} />
    </div>
  );
};

const CenteredState = ({ children }: { children: React.ReactNode }) => (
  <div className='flex min-h-0 flex-1 items-center justify-center p-12 text-center text-muted-foreground text-xs'>
    {children}
  </div>
);

const EmptyState = ({ onCreate }: { onCreate: () => void }) => (
  <div className='flex flex-col items-center gap-2'>
    <AppWindow className='size-6 text-muted-foreground/60' />
    <p>No custom apps yet.</p>
    <Button size='sm' variant='outline' onClick={onCreate}>
      <Plus className='size-3.5' />
      Create the first
    </Button>
  </div>
);
