import { useEffect, useMemo } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import useCurrentWorkspace from "./useCurrentWorkspace";

interface DatabaseConnection {
  id: string;
  name: string;
  type: string;
  host?: string;
  port?: string;
  database?: string;
  username?: string;
  schemas?: DatabaseSchema[];
  isConnected?: boolean;
  synced?: boolean;
}

interface DatabaseSchema {
  name: string;
  tables: DatabaseTable[];
  views?: DatabaseView[];
}

interface DatabaseTable {
  name: string;
  columns?: TableColumn[];
}

interface DatabaseView {
  name: string;
  columns?: TableColumn[];
}

interface TableColumn {
  name: string;
  type: string;
  nullable?: boolean;
  primaryKey?: boolean;
}

/**
 * Structured details for a failed SQL execution as returned by the
 * `/sql/query` endpoint. `message` is always present; the remaining fields
 * are populated when the underlying connector exposes vendor metadata
 * (Postgres SQLSTATE / DETAIL / HINT / POSITION).
 */
export interface SqlExecutionError {
  message: string;
  code?: string;
  detail?: string;
  hint?: string;
  /** 1-based character offset into the failing SQL. */
  position?: number;
  /** Echoed only when the connector executed a different SQL than the user typed. */
  sql?: string;
}

export interface QueryTab {
  id: string;
  name: string;
  content: string;
  connectionId?: string;
  selectedDatabase?: string | null;
  isDirty: boolean;
  savedPath?: string;
  results?: QueryResult;
  isExecuting?: boolean;
  error?: string;
  /**
   * Structured SQL execution error. Set alongside `error` for query failures
   * so the IDE can render a structured block (SQLSTATE badge, hint, detail,
   * position) without sacrificing the plain-text fallback.
   */
  errorDetails?: SqlExecutionError;
}

interface QueryResult {
  result: string[][];
  resultFile: string | undefined;
  executionTime?: number;
  /** Result hit the ad-hoc row cap (10k); more rows exist in the warehouse. */
  truncated?: boolean;
}

/** Everything the SQL console tracks for a single workspace. */
interface WorkspaceDatabaseState {
  connections: DatabaseConnection[];
  activeConnectionId: string | null;
  tabs: QueryTab[];
  activeTabId: string | null;
}

const emptyWorkspaceState = (): WorkspaceDatabaseState => ({
  connections: [],
  activeConnectionId: null,
  tabs: [],
  activeTabId: null
});

// Stable empty-state reference used as the selector fallback so components
// reading an as-yet-unseen workspace id don't re-render on every store tick.
const EMPTY_WORKSPACE_STATE = emptyWorkspaceState();

interface DatabaseClientState {
  // Keyed by workspace id — query tabs, drafts, and connection selection are
  // workspace-scoped data, not global. A flat/shared shape here previously
  // let a query tab (and its raw SQL) written in one workspace keep
  // rendering after switching to another workspace, since switching only
  // changes the `:wsId` route param and doesn't remount the IDE (see #2962).
  byWorkspace: Record<string, WorkspaceDatabaseState>;
  // A pre-#2962 flat SQL-tab blob salvaged from a v0 persisted store, not
  // yet attributed to any workspace (the old shape had no workspace id to
  // attribute it to). Adopted into the first workspace whose SQL console
  // mounts after the upgrade (`adoptLegacy`, called from `useDatabaseClient`),
  // then cleared. Without this, upgrading silently drops every open tab and
  // its unsaved SQL the first time the store rehydrates into the new shape.
  legacy: WorkspaceDatabaseState | null;
  adoptLegacy: (wsId: string) => void;

  setActiveConnection: (wsId: string, id: string | null) => void;

  // Tab actions
  addTab: (
    wsId: string,
    tab?: Partial<Omit<QueryTab, "id">>
  ) => {
    success: boolean;
    tabId?: string;
    error?: string;
  };
  updateTab: (wsId: string, id: string, updates: Partial<QueryTab>) => void;
  removeTab: (wsId: string, id: string) => void;
  setActiveTab: (wsId: string, id: string | null) => void;
  tabExists: (wsId: string, name: string) => boolean;
  getUniqueTabName: (wsId: string, baseName?: string) => string;

  /**
   * The active tab, read on call rather than subscribed to. Lets a callback
   * that only needs the tab *at invocation time* (the Cmd+Enter run-query
   * handler) keep a stable identity across keystrokes — `tabs` is rebuilt by
   * `.map()` on every content update, so depending on the tab object churns.
   */
  getActiveTab: (wsId: string) => QueryTab | undefined;

  setTabExecuting: (wsId: string, id: string, isExecuting: boolean) => void;
  setTabResults: (wsId: string, id: string, results: QueryResult | undefined) => void;
  setTabError: (
    wsId: string,
    id: string,
    error: string | undefined,
    details?: SqlExecutionError
  ) => void;

  // Sync with external file editors
  getTabByPath: (wsId: string, path: string) => QueryTab | undefined;
  updateTabByPath: (wsId: string, path: string, content: string) => void;
}

// eslint-disable-next-line sonarjs/pseudo-random
const generateId = () => Math.random().toString(36).substring(2, 11);

const useDatabaseClientStore = create<DatabaseClientState>()(
  persist(
    (set, get) => {
      const getSlice = (wsId: string) => get().byWorkspace[wsId] ?? EMPTY_WORKSPACE_STATE;
      const updateSlice = (
        wsId: string,
        updater: (slice: WorkspaceDatabaseState) => WorkspaceDatabaseState
      ) =>
        set((state) => ({
          byWorkspace: {
            ...state.byWorkspace,
            [wsId]: updater(state.byWorkspace[wsId] ?? emptyWorkspaceState())
          }
        }));

      return {
        byWorkspace: {},
        legacy: null,

        adoptLegacy: (wsId) => {
          set((state) => {
            if (!state.legacy || state.byWorkspace[wsId]) return state;
            return {
              byWorkspace: { ...state.byWorkspace, [wsId]: state.legacy },
              legacy: null
            };
          });
        },

        setActiveConnection: (wsId, id) => {
          updateSlice(wsId, (slice) => ({ ...slice, activeConnectionId: id }));
        },

        // Tab actions
        addTab: (wsId, tab) => {
          const { tabExists, getUniqueTabName } = get();
          const name = tab?.name || getUniqueTabName(wsId);

          // Check for existing tab with same name if a specific name was provided
          if (tab?.name && tabExists(wsId, tab.name)) {
            const existingTab = getSlice(wsId).tabs.find(
              (t) => t.name.toLowerCase() === tab.name?.toLowerCase()
            );
            if (existingTab) {
              updateSlice(wsId, (slice) => ({ ...slice, activeTabId: existingTab.id }));
            }
            return {
              success: false,
              error: `Tab "${tab.name}" already exists`
            };
          }

          const newTab: QueryTab = {
            id: generateId(),
            name,
            content: tab?.content || "",
            connectionId: tab?.connectionId,
            selectedDatabase: tab?.selectedDatabase ?? null,
            isDirty: tab?.isDirty || false,
            savedPath: tab?.savedPath,
            results: tab?.results,
            isExecuting: false,
            error: undefined
          };

          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: [...slice.tabs, newTab],
            activeTabId: newTab.id
          }));

          return { success: true, tabId: newTab.id };
        },

        updateTab: (wsId, id, updates) => {
          const computeIsDirty = (tab: QueryTab): boolean => {
            if (updates.isDirty !== undefined) {
              return updates.isDirty;
            }
            if (updates.content !== undefined) {
              return updates.content !== tab.content;
            }
            return tab.isDirty;
          };

          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: slice.tabs.map((t) =>
              t.id === id
                ? {
                    ...t,
                    ...updates,
                    isDirty: computeIsDirty(t)
                  }
                : t
            )
          }));
        },

        removeTab: (wsId, id) => {
          const { tabs, activeTabId } = getSlice(wsId);
          const tabIndex = tabs.findIndex((t) => t.id === id);
          const newTabs = tabs.filter((t) => t.id !== id);

          let newActiveTabId = activeTabId;
          if (activeTabId === id) {
            if (newTabs.length > 0) {
              // Select the tab to the left, or the first tab
              const newIndex = Math.max(0, tabIndex - 1);
              newActiveTabId = newTabs[newIndex]?.id || null;
            } else {
              newActiveTabId = null;
            }
          }

          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: newTabs,
            activeTabId: newActiveTabId
          }));
        },

        setActiveTab: (wsId, id) => {
          updateSlice(wsId, (slice) => ({ ...slice, activeTabId: id }));
        },

        tabExists: (wsId, name) => {
          return getSlice(wsId).tabs.some((t) => t.name.toLowerCase() === name.toLowerCase());
        },

        getUniqueTabName: (wsId, baseName = "Untitled") => {
          const { tabs } = getSlice(wsId);
          let counter = 1;
          let name = `${baseName}-${counter}.sql`;

          while (tabs.some((t) => t.name.toLowerCase() === name.toLowerCase())) {
            counter++;
            name = `${baseName}-${counter}.sql`;
          }

          return name;
        },

        getActiveTab: (wsId) => {
          const { tabs, activeTabId } = getSlice(wsId);
          return tabs.find((t) => t.id === activeTabId);
        },

        // Query execution
        setTabExecuting: (wsId, id, isExecuting) => {
          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: slice.tabs.map((t) =>
              t.id === id ? { ...t, isExecuting, error: undefined, errorDetails: undefined } : t
            )
          }));
        },

        setTabResults: (wsId, id, results) => {
          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: slice.tabs.map((t) => (t.id === id ? { ...t, results, isExecuting: false } : t))
          }));
        },

        setTabError: (wsId, id, error, details) => {
          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: slice.tabs.map((t) =>
              t.id === id ? { ...t, error, errorDetails: details, isExecuting: false } : t
            )
          }));
        },

        // Sync with external file editors
        getTabByPath: (wsId, path) => {
          return getSlice(wsId).tabs.find((t) => t.savedPath === path);
        },

        updateTabByPath: (wsId, path, content) => {
          updateSlice(wsId, (slice) => ({
            ...slice,
            tabs: slice.tabs.map((t) =>
              t.savedPath === path ? { ...t, content, isDirty: false } : t
            )
          }));
        }
      };
    },
    {
      name: "database-client-storage",
      // v0 persisted a flat {connections, tabs, activeConnectionId,
      // activeTabId} blob; v1 namespaces it by workspace id (#2962). zustand's
      // default `merge` is a shallow `{...current, ...persisted}`, so without
      // this migration the v0 blob's keys just ride along unread and
      // `byWorkspace` starts empty — every saved query tab silently
      // disappears on first load after the upgrade. Salvage it into `legacy`
      // instead so `adoptLegacy` can recover it into whichever workspace's
      // SQL console mounts first.
      version: 1,
      migrate: (persisted, version) => {
        if (version === 0 && persisted && typeof persisted === "object") {
          const v0 = persisted as Partial<WorkspaceDatabaseState>;
          const hasContent = (v0.tabs?.length ?? 0) > 0 || (v0.connections?.length ?? 0) > 0;
          return {
            byWorkspace: {},
            legacy: hasContent
              ? {
                  connections: v0.connections ?? [],
                  activeConnectionId: v0.activeConnectionId ?? null,
                  tabs: v0.tabs ?? [],
                  activeTabId: v0.activeTabId ?? null
                }
              : null
          } as DatabaseClientState;
        }
        return persisted as DatabaseClientState;
      },
      partialize: (state) => {
        const scrub = (slice: WorkspaceDatabaseState): WorkspaceDatabaseState => ({
          connections: slice.connections.map((c) => ({
            ...c,
            isConnected: false,
            schemas: undefined
          })),
          tabs: slice.tabs.map((t) => ({
            ...t,
            results: undefined,
            isExecuting: false,
            error: undefined,
            errorDetails: undefined
          })),
          activeConnectionId: slice.activeConnectionId,
          activeTabId: slice.activeTabId
        });

        return {
          // Drop empty slices so a workspace briefly opened once doesn't
          // hold a permanent (if empty) spot in localStorage forever.
          byWorkspace: Object.fromEntries(
            Object.entries(state.byWorkspace)
              .filter(([, slice]) => slice.tabs.length > 0)
              .map(([wsId, slice]) => [wsId, scrub(slice)])
          ),
          legacy: state.legacy ? scrub(state.legacy) : null
        };
      }
    }
  )
);

/**
 * The IDE SQL console's per-workspace state (query tabs, active tab,
 * connection selection). Resolves the current workspace id itself so every
 * caller keeps the flat call shape (`addTab(tab)`, not `addTab(wsId, tab)`)
 * while the underlying store stays namespaced by workspace.
 */
export default function useDatabaseClient() {
  const wsId = useCurrentWorkspace((s) => s.workspace?.id) ?? "";

  // Recover a pre-#2962 v0 blob (see the `migrate` above) into whichever
  // workspace's SQL console mounts first after the upgrade. A no-op once
  // `legacy` has been adopted (or there was none to adopt).
  useEffect(() => {
    if (!wsId) return;
    useDatabaseClientStore.getState().adoptLegacy(wsId);
  }, [wsId]);

  const slice = useDatabaseClientStore((s) => s.byWorkspace[wsId] ?? EMPTY_WORKSPACE_STATE);

  // Bound to `wsId` only (not recreated on every store update) so callers
  // that put these in a `useCallback`/`useEffect` dep array — e.g. the
  // Monaco Cmd+Enter run-query keybinding — don't get their identity
  // churned, and the keybinding re-registered, on every keystroke.
  const actions = useMemo(
    () => ({
      setActiveConnection: (id: string | null) =>
        useDatabaseClientStore.getState().setActiveConnection(wsId, id),
      addTab: (tab?: Partial<Omit<QueryTab, "id">>) =>
        useDatabaseClientStore.getState().addTab(wsId, tab),
      updateTab: (id: string, updates: Partial<QueryTab>) =>
        useDatabaseClientStore.getState().updateTab(wsId, id, updates),
      removeTab: (id: string) => useDatabaseClientStore.getState().removeTab(wsId, id),
      setActiveTab: (id: string | null) => useDatabaseClientStore.getState().setActiveTab(wsId, id),
      tabExists: (name: string) => useDatabaseClientStore.getState().tabExists(wsId, name),
      getUniqueTabName: (baseName?: string) =>
        useDatabaseClientStore.getState().getUniqueTabName(wsId, baseName),
      getActiveTab: () => useDatabaseClientStore.getState().getActiveTab(wsId),
      setTabExecuting: (id: string, isExecuting: boolean) =>
        useDatabaseClientStore.getState().setTabExecuting(wsId, id, isExecuting),
      setTabResults: (id: string, results: QueryResult | undefined) =>
        useDatabaseClientStore.getState().setTabResults(wsId, id, results),
      setTabError: (id: string, error: string | undefined, details?: SqlExecutionError) =>
        useDatabaseClientStore.getState().setTabError(wsId, id, error, details),
      getTabByPath: (path: string) => useDatabaseClientStore.getState().getTabByPath(wsId, path),
      updateTabByPath: (path: string, content: string) =>
        useDatabaseClientStore.getState().updateTabByPath(wsId, path, content)
    }),
    [wsId]
  );

  return { ...slice, ...actions };
}
