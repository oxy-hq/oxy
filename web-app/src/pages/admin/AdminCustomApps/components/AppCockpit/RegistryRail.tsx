import { ChevronRight, Search } from "lucide-react";
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { cn } from "@/libs/shadcn/utils";
import type { CustomApp } from "@/types/apps";
import {
  type AppsTableState,
  buildAppsTableModel,
  useAppsTableState
} from "../AppsTable/useAppsTable";
import { RegistryRow } from "./RegistryRow";

interface RegistryRailProps {
  apps: CustomApp[];
  selected: CustomApp | null;
  onSelect: (app: CustomApp) => void;
  onPublish: (app: CustomApp) => void;
  onUnpublish: (app: CustomApp) => void;
}

/**
 * The cockpit spine: a filterable, keyboard-navigable list of every app, always
 * beside the stage so switching apps never round-trips through the landing.
 * Reuses the landing's URL-synced filter/sort/group state (so a filter set here
 * carries there and back) and the same `buildAppsTableModel` transform. ↑/↓ move
 * the selection through the *visible* order (collapsed groups excluded) and pull
 * the active row into view.
 */
export const RegistryRail = ({
  apps,
  selected,
  onSelect,
  onPublish,
  onUnpublish
}: RegistryRailProps) => {
  const [state, setState] = useAppsTableState();
  const model = useMemo(() => buildAppsTableModel(apps, state), [apps, state]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const activeRef = useRef<HTMLButtonElement>(null);
  // Set just before a keyboard-driven selection so the effect knows to move DOM
  // focus with the selection (not on mouse clicks, which shouldn't steal focus).
  const keyboardNav = useRef(false);

  const byId = useMemo(() => new Map(apps.map((a) => [a.id, a])), [apps]);
  const visibleIds = useMemo(
    () =>
      model.groups.filter((g) => !collapsed.has(g.key)).flatMap((g) => g.items.map((a) => a.id)),
    [model.groups, collapsed]
  );

  // Keep the selected row on screen as ↑/↓ walks past the fold, and — when the
  // change came from the keyboard — move focus onto it so focus and selection
  // never diverge (otherwise the hover card can pop for the stale focused row).
  // The selection id is a trigger, not a value read in the body.
  // biome-ignore lint/correctness/useExhaustiveDependencies: selection id drives the scroll/focus
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest" });
    if (keyboardNav.current) {
      activeRef.current?.focus();
      keyboardNav.current = false;
    }
  }, [selected?.id]);

  const step = (dir: 1 | -1) => {
    if (visibleIds.length === 0) return;
    const idx = selected ? visibleIds.indexOf(selected.id) : -1;
    const next = idx === -1 ? (dir === 1 ? 0 : visibleIds.length - 1) : idx + dir;
    const clamped = Math.max(0, Math.min(visibleIds.length - 1, next));
    const app = byId.get(visibleIds[clamped]);
    if (app) onSelect(app);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      keyboardNav.current = true;
      step(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      keyboardNav.current = true;
      step(-1);
    }
  };

  const toggle = (key: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const showOrg = state.group !== "org";
  const showHeaders = state.group !== "none";

  return (
    <div className='flex h-full min-h-0 flex-col bg-sidebar-background/40'>
      <div className='flex shrink-0 items-center gap-1.5 border-b p-2'>
        <div className='relative min-w-0 flex-1'>
          <Search className='absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground' />
          <Input
            value={state.q}
            onChange={(e) => setState({ q: e.target.value })}
            placeholder='Filter apps…'
            className='h-7 pl-7 text-xs'
          />
        </div>
        <Select
          value={state.group}
          onValueChange={(v) => setState({ group: v as AppsTableState["group"] })}
        >
          <SelectTrigger className='h-7 w-auto gap-1 px-2 text-xs' aria-label='Group by'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent align='end'>
            <SelectItem value='org'>Org</SelectItem>
            <SelectItem value='status'>Status</SelectItem>
            <SelectItem value='source'>Source</SelectItem>
            <SelectItem value='none'>Flat</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* biome-ignore lint/a11y/noStaticElementInteractions: keyboard handler is
          a roving-selection convenience over the child buttons, which remain the
          real, individually-focusable controls. */}
      <div className='min-h-0 flex-1 overflow-y-auto p-1.5' onKeyDown={onKeyDown}>
        {model.filteredCount === 0 ? (
          <p className='px-2 py-6 text-center text-muted-foreground text-xs'>No apps match.</p>
        ) : (
          model.groups.map((group) => {
            const isCollapsed = collapsed.has(group.key);
            return (
              <div key={group.key} className='mb-1'>
                {showHeaders && (
                  <button
                    type='button'
                    onClick={() => toggle(group.key)}
                    className='flex w-full items-center gap-1 rounded px-2 py-1 text-left text-muted-foreground hover:text-foreground'
                  >
                    <ChevronRight
                      className={cn("size-3 transition-transform", !isCollapsed && "rotate-90")}
                    />
                    <span className='min-w-0 flex-1 truncate font-medium text-[10px] uppercase tracking-wider'>
                      {group.label}
                    </span>
                    <span className='font-mono text-[10px] tabular-nums'>{group.items.length}</span>
                  </button>
                )}
                {!isCollapsed &&
                  group.items.map((app) => {
                    const isSel = selected?.id === app.id;
                    return (
                      <RegistryRow
                        key={app.id}
                        ref={isSel ? activeRef : undefined}
                        app={app}
                        selected={isSel}
                        showOrg={showOrg}
                        onSelect={onSelect}
                        onPublish={onPublish}
                        onUnpublish={onUnpublish}
                      />
                    );
                  })}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
