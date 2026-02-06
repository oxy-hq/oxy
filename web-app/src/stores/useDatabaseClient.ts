import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface DatabaseConnection {
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

export interface DatabaseSchema {
  name: string;
  tables: DatabaseTable[];
  views?: DatabaseView[];
}

export interface DatabaseTable {
  name: string;
  columns?: TableColumn[];
}

export interface DatabaseView {
  name: string;
  columns?: TableColumn[];
}

export interface TableColumn {
  name: string;
  type: string;
  nullable?: boolean;
  primaryKey?: boolean;
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
}

export interface QueryResult {
  result: string[][];
  resultFile: string | undefined;
  executionTime?: number;
}

interface DatabaseClientState {
  // Connections
  connections: DatabaseConnection[];
  activeConnectionId: string | null;

  // Query tabs
  tabs: QueryTab[];
  activeTabId: string | null;
  setActiveConnection: (id: string | null) => void;

  // Tab actions
  addTab: (tab?: Partial<Omit<QueryTab, "id">>) => {
    success: boolean;
    tabId?: string;
    error?: string;
  };
  updateTab: (id: string, updates: Partial<QueryTab>) => void;
  removeTab: (id: string) => void;
  setActiveTab: (id: string | null) => void;
  tabExists: (name: string) => boolean;
  getUniqueTabName: (baseName?: string) => string;

  // Query execution
  setTabExecuting: (id: string, isExecuting: boolean) => void;
  setTabResults: (id: string, results: QueryResult | undefined) => void;
  setTabError: (id: string, error: string | undefined) => void;

  // Sync with external file editors
  getTabByPath: (path: string) => QueryTab | undefined;
  updateTabByPath: (path: string, content: string) => void;
}

// eslint-disable-next-line sonarjs/pseudo-random
const generateId = () => Math.random().toString(36).substring(2, 11);

const useDatabaseClientStore = create<DatabaseClientState>()(
  persist(
    (set, get) => ({
      connections: [],
      activeConnectionId: null,
      tabs: [],
      activeTabId: null,

      setActiveConnection: (id) => {
        set({ activeConnectionId: id });
      },

      // Tab actions
      addTab: (tab) => {
        const { getUniqueTabName } = get();
        const name = tab?.name || getUniqueTabName();

        // Check for existing tab with same name if a specific name was provided
        if (tab?.name && get().tabExists(tab.name)) {
          const existingTab = get().tabs.find(
            (t) => t.name.toLowerCase() === tab.name?.toLowerCase()
          );
          if (existingTab) {
            set({ activeTabId: existingTab.id });
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

        set((state) => ({
          tabs: [...state.tabs, newTab],
          activeTabId: newTab.id
        }));

        return { success: true, tabId: newTab.id };
      },

      updateTab: (id, updates) => {
        const computeIsDirty = (tab: QueryTab): boolean => {
          if (updates.isDirty !== undefined) {
            return updates.isDirty;
          }
          if (updates.content !== undefined) {
            return updates.content !== tab.content;
          }
          return tab.isDirty;
        };

        set((state) => ({
          tabs: state.tabs.map((t) =>
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

      removeTab: (id) => {
        const { tabs, activeTabId } = get();
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

        set({
          tabs: newTabs,
          activeTabId: newActiveTabId
        });
      },

      setActiveTab: (id) => {
        set({ activeTabId: id });
      },

      tabExists: (name) => {
        const { tabs } = get();
        return tabs.some((t) => t.name.toLowerCase() === name.toLowerCase());
      },

      getUniqueTabName: (baseName = "Untitled") => {
        const { tabs } = get();
        let counter = 1;
        let name = `${baseName}-${counter}.sql`;

        while (tabs.some((t) => t.name.toLowerCase() === name.toLowerCase())) {
          counter++;
          name = `${baseName}-${counter}.sql`;
        }

        return name;
      },

      // Query execution
      setTabExecuting: (id, isExecuting) => {
        set((state) => ({
          tabs: state.tabs.map((t) => (t.id === id ? { ...t, isExecuting, error: undefined } : t))
        }));
      },

      setTabResults: (id, results) => {
        set((state) => ({
          tabs: state.tabs.map((t) => (t.id === id ? { ...t, results, isExecuting: false } : t))
        }));
      },

      setTabError: (id, error) => {
        set((state) => ({
          tabs: state.tabs.map((t) => (t.id === id ? { ...t, error, isExecuting: false } : t))
        }));
      },

      // Sync with external file editors
      getTabByPath: (path) => {
        const { tabs } = get();
        return tabs.find((t) => t.savedPath === path);
      },

      updateTabByPath: (path, content) => {
        set((state) => ({
          tabs: state.tabs.map((t) =>
            t.savedPath === path ? { ...t, content, isDirty: false } : t
          )
        }));
      }
    }),
    {
      name: "database-client-storage",
      partialize: (state) => ({
        connections: state.connections.map((c) => ({
          ...c,
          isConnected: false,
          schemas: undefined
        })),
        tabs: state.tabs.map((t) => ({
          ...t,
          results: undefined,
          isExecuting: false,
          error: undefined
        })),
        activeConnectionId: state.activeConnectionId,
        activeTabId: state.activeTabId
      })
    }
  )
);

export default function useDatabaseClient() {
  return useDatabaseClientStore();
}
