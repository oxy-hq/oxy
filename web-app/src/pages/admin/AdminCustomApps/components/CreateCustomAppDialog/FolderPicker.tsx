import { ChevronRight, Folder, FolderOpen, Home, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useListdir } from "@/hooks/api/customApps/useListdir";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  /** Absolute path of the currently-selected folder, or empty when none. */
  value: string;
  onChange: (path: string) => void;
};

/**
 * Server-side folder picker for the local-mode "Link existing" flow.
 *
 * Why server-side: typing absolute paths is brittle (we've hit `~`
 * non-expansion, trailing-slash typos, symlink confusion). The user
 * navigates by clicking, then the dialog submits the exact `path`
 * the server reported — no string typing.
 *
 * Layout: a single mono breadcrumb at the top + a scrolling entry
 * list below. The breadcrumb shows the absolute path the server
 * resolved to; clicking a segment jumps there. The entry list shows
 * dirs first (selectable), then files (greyed out). A "..." entry
 * for the parent is always at the top when we're not at root.
 *
 * The "selected" affordance is a left border + bg so the user can
 * see exactly which folder they're about to link without having to
 * re-check the breadcrumb. Single click = navigate AND select; the
 * breadcrumb at the top is the canonical selection display.
 */
export const FolderPicker = ({ value, onChange }: Props) => {
  // Track the path being browsed independently of the selected `value`.
  // They're equal in 99% of cases, but the user might want to peek into
  // a deeper folder without changing the selection.
  //
  // Initial cwd priority: parent-supplied value > last-used folder from
  // localStorage > empty (server picks default). Most admins link
  // multiple bundles from sibling folders under the same repo root —
  // remembering "where you were" turns the second link into one click
  // instead of re-navigating from `$HOME`.
  const [cwd, setCwd] = useState(() => value || readRememberedFolder());
  const [showHidden, setShowHidden] = useState(false);

  const listing = useListdir(cwd, showHidden);

  // First mount with empty path: server returns its chosen default
  // (`$OXY_STATE_DIR/customer-apps` or `$HOME`). Sync that back as
  // the selected value so the user can submit without ever clicking
  // — the picker landing is the most useful "ok" choice.
  if (listing.data && !value) {
    onChange(listing.data.path);
    setCwd(listing.data.path);
  }

  // Persist the current selection so the next dialog open lands here.
  // Effect (not render-phase) so we don't write during render. Skips
  // empty strings — a Home-button reset shouldn't poison the memory.
  useEffect(() => {
    if (value) rememberFolder(value);
  }, [value]);

  const navigate = (path: string) => {
    setCwd(path);
    onChange(path);
  };

  const visiblePath = listing.data?.path ?? cwd;
  const breadcrumbs = pathSegments(visiblePath);

  // Auto-scroll the breadcrumb to its right edge whenever the path
  // changes — long paths (`/Users/foo/oxy-hq/.../bundle`) otherwise
  // park at the root segment and hide the actual current folder.
  // Operators care about the deepest segment, so make that the
  // initially-visible part of the row. visiblePath is the trigger
  // even though the effect body reads only from the ref — that's
  // intentional, biome's rule misreads it as redundant.
  const breadcrumbRef = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: visiblePath is the change trigger we want to react to
  useEffect(() => {
    const el = breadcrumbRef.current;
    if (el) el.scrollLeft = el.scrollWidth;
  }, [visiblePath]);

  return (
    // min-w-0 here + on every nested flex row prevents the
    // intrinsic content width of a long absolute path (which the
    // breadcrumb wants to render whole) from blowing past the
    // picker's column allotment in the dialog. The breadcrumb's
    // own overflow-x-auto only kicks in once its parent stops
    // sizing to content — so the constraint has to propagate all
    // the way up the chain.
    <div className='flex min-w-0 flex-col gap-2 rounded-md border border-border'>
      <div className='flex min-w-0 items-center gap-1 border-border border-b bg-muted/40 px-2 py-1.5'>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='size-7'
          onClick={() => navigate("")}
          title='Default folder'
        >
          <Home className='size-3.5' />
        </Button>
        <div
          ref={breadcrumbRef}
          className='flex min-w-0 flex-1 items-center overflow-x-auto whitespace-nowrap font-mono text-xs'
        >
          {breadcrumbs.map((seg, i) => (
            <span key={seg.path} className='flex shrink-0 items-center'>
              {i > 0 && <ChevronRight className='size-3 shrink-0 text-muted-foreground' />}
              <button
                type='button'
                onClick={() => navigate(seg.path)}
                className='shrink-0 rounded px-1 py-0.5 hover:bg-muted'
              >
                {seg.label}
              </button>
            </span>
          ))}
        </div>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='size-7'
          onClick={() => listing.refetch()}
          title='Refresh'
        >
          <RefreshCw className={cn("size-3.5", listing.isFetching && "animate-spin")} />
        </Button>
      </div>

      <div className='max-h-64 overflow-y-auto'>
        {listing.isLoading && (
          <div className='flex items-center justify-center py-8'>
            <Spinner />
          </div>
        )}

        {listing.error && (
          <div className='px-3 py-4 text-destructive text-xs'>
            {listing.error instanceof Error ? listing.error.message : "Couldn't read this folder."}
          </div>
        )}

        {listing.data && (
          <ul className='flex flex-col py-1'>
            {listing.data.parent !== null && (
              <EntryRow
                onClick={() => navigate(listing.data.parent as string)}
                icon={<FolderOpen className='size-3.5 text-muted-foreground' />}
                label='..'
                selectable
              />
            )}
            {listing.data.entries.length === 0 && !listing.data.parent && (
              <li className='px-3 py-2 text-muted-foreground text-xs'>Empty folder.</li>
            )}
            {listing.data.entries.map((entry) => (
              <EntryRow
                key={entry.path}
                onClick={() => entry.is_dir && navigate(entry.path)}
                icon={
                  <Folder
                    className={cn(
                      "size-3.5",
                      entry.is_dir ? "text-primary" : "text-muted-foreground/40"
                    )}
                  />
                }
                label={entry.name}
                selected={entry.is_dir && entry.path === value}
                selectable={entry.is_dir}
              />
            ))}
          </ul>
        )}
      </div>

      <div className='flex items-center gap-3 border-border border-t px-2 py-1.5'>
        <label className='flex shrink-0 items-center gap-1.5 text-muted-foreground text-xs'>
          <input
            type='checkbox'
            className='size-3'
            checked={showHidden}
            onChange={(e) => setShowHidden(e.target.checked)}
          />
          Show hidden
        </label>
        {/* min-w-0 lets `truncate` actually clamp inside the flex row —
            without it the span insists on its content-width and the
            whole row blows out past the dialog when the path is long. */}
        <span
          className='min-w-0 flex-1 truncate text-right font-mono text-muted-foreground text-xs'
          title={value || undefined}
        >
          {value || "No folder selected"}
        </span>
      </div>
    </div>
  );
};

type EntryRowProps = {
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  selected?: boolean;
  selectable?: boolean;
};

const EntryRow = ({ onClick, icon, label, selected, selectable }: EntryRowProps) => (
  <li>
    <button
      type='button'
      onClick={onClick}
      disabled={!selectable}
      data-selected={selected ? "true" : undefined}
      className={cn(
        "flex w-full items-center gap-2 border-transparent border-l-2 px-2 py-1 text-left text-sm",
        selectable ? "hover:bg-muted" : "cursor-default text-muted-foreground/60",
        "data-[selected=true]:border-primary data-[selected=true]:bg-primary/5"
      )}
    >
      {icon}
      <span className='truncate font-mono text-xs'>{label}</span>
    </button>
  </li>
);

// localStorage key for remembering the last-selected folder across
// dialog opens. Per-browser, not per-org — operators tend to work on
// one org's bundles at a time and rarely jump between project roots
// in a single session, so the simpler single-key model wins over a
// per-org partitioning that would clutter storage.
const REMEMBERED_FOLDER_KEY = "oxy.adminCustomApps.lastLocalFolder";

const readRememberedFolder = (): string => {
  if (typeof window === "undefined") return "";
  try {
    return window.localStorage.getItem(REMEMBERED_FOLDER_KEY) ?? "";
  } catch {
    // localStorage can throw in private-browsing mode or when the
    // origin's storage is full / disabled by policy. Treat as
    // "no memory" and fall back to the server default.
    return "";
  }
};

const rememberFolder = (path: string): void => {
  if (typeof window === "undefined" || !path) return;
  try {
    window.localStorage.setItem(REMEMBERED_FOLDER_KEY, path);
  } catch {
    // Same swallow as readRememberedFolder — not surfacing this is
    // deliberate, the picker still works without persistence.
  }
};

/**
 * Split an absolute path into clickable segments. For `/a/b/c`, returns
 * `[{label: "/", path: "/"}, {label: "a", path: "/a"}, …]` so the
 * breadcrumb can jump back to any ancestor.
 */
const pathSegments = (path: string): Array<{ label: string; path: string }> => {
  if (!path) return [];
  const trimmed = path.replace(/\/+$/, "");
  if (trimmed === "" || trimmed === "/") return [{ label: "/", path: "/" }];
  const parts = trimmed.split("/").filter(Boolean);
  const out: Array<{ label: string; path: string }> = [{ label: "/", path: "/" }];
  let acc = "";
  for (const part of parts) {
    acc += `/${part}`;
    out.push({ label: part, path: acc });
  }
  return out;
};
