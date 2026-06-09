import { useState } from "react";
import { toast } from "sonner";
import { useDiscoverSourceTables } from "@/hooks/api/airway/useAirway";
import { useCreateSecret } from "@/hooks/api/secrets/useSecretMutations";
import type { DiscoveredTable } from "@/services/api/airway";
import type { ClickHouseScaffoldTable, WriteDisposition } from "../../../scaffold";

/** Per-table load config the user sets in the picker. */
export interface TableSel {
  disposition: WriteDisposition;
  /** High-water-mark column for incremental `append`. */
  cursorField?: string;
  /** Upsert key for `merge`. */
  primaryKey?: string;
}

export interface ClickHouseScaffoldConfig {
  host: string;
  port?: number;
  database: string;
  username?: string;
  passwordVar?: string;
  secure?: boolean;
  tables: ClickHouseScaffoldTable[];
}

/** Owns the ClickHouse connection form state, table discovery + per-table
 *  load configuration, validation, secret persistence and scaffold-config
 *  shaping for the New Pipeline wizard. */
export function useClickHouseCredentials() {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [database, setDatabase] = useState("default");
  const [username, setUsername] = useState("default");
  const [password, setPassword] = useState("");
  const [secretName, setSecretName] = useState("");
  const [secure, setSecure] = useState(true);
  // Tables returned by discovery (with columns), and per-table load config
  // keyed by table name (presence == selected).
  const [tables, setTables] = useState<DiscoveredTable[] | null>(null);
  const [selection, setSelection] = useState<Record<string, TableSel>>({});
  const discoverTables = useDiscoverSourceTables();
  const createSecret = useCreateSecret();

  const reset = () => {
    setHost("");
    setPort("");
    setDatabase("default");
    setUsername("default");
    setPassword("");
    setSecretName("");
    setSecure(true);
    setTables(null);
    setSelection({});
  };

  const parsedPort = (): number | undefined => {
    const p = port.trim();
    if (!p) return undefined;
    const n = Number(p);
    return Number.isInteger(n) && n > 0 ? n : undefined;
  };

  /** Live credentials for discovery + scaffold. Password included only
   *  when typed (discovery needs it; reuse-existing-secret can't list). */
  const liveConfig = (): Record<string, unknown> => {
    const config: Record<string, unknown> = {
      host: host.trim(),
      database: database.trim(),
      username: username.trim() || "default",
      secure
    };
    const p = parsedPort();
    if (p != null) config.port = p;
    if (password) config.password = password;
    return config;
  };

  const fetchTables = async (onError: (message: string | null) => void) => {
    if (!host.trim() || !database.trim()) {
      onError("ClickHouse: host and database are required to list tables");
      return;
    }
    onError(null);
    try {
      const discovered = await discoverTables.mutateAsync({
        kind: "clickhouse",
        config: liveConfig()
      });
      setTables(discovered);
      setSelection({});
    } catch (err) {
      toast.error("Couldn't list tables", {
        description: err instanceof Error ? err.message : "Check the credentials and try again."
      });
    }
  };

  const toggleTable = (name: string) => {
    setSelection((prev) => {
      const next = { ...prev };
      if (name in next) delete next[name];
      else next[name] = { disposition: "append" };
      return next;
    });
  };

  const setTableConfig = (name: string, patch: Partial<TableSel>) => {
    setSelection((prev) =>
      name in prev ? { ...prev, [name]: { ...prev[name], ...patch } } : prev
    );
  };

  const selectedCount = Object.keys(selection).length;
  const allTablesSelected = tables != null && tables.length > 0 && selectedCount === tables.length;

  const toggleSelectAll = () => {
    if (!tables) return;
    setSelection((prev) => {
      if (allTablesSelected) return {};
      const next = { ...prev };
      for (const t of tables) if (!(t.name in next)) next[t.name] = { disposition: "append" };
      return next;
    });
  };

  /** Returns an error message, or null when the form is valid. */
  const validate = (): string | null => {
    if (!host.trim() || !database.trim()) return "ClickHouse: host and database are required";
    if (!secretName.trim()) return "ClickHouse: a secret name for the password is required";
    if (!password) {
      // Discovery uses the live password; clearing it before Create would
      // leave a `password_var` reference with no secret behind it.
      return "ClickHouse: password is required";
    }
    if (selectedCount === 0) return "ClickHouse: select at least one table to ingest";
    const mergeNoKey = Object.entries(selection).find(
      ([, sel]) => sel.disposition === "merge" && !sel.primaryKey
    );
    if (mergeNoKey)
      return `ClickHouse: pick a merge key for "${mergeNoKey[0]}" (or change its load mode)`;
    return null;
  };

  const buildScaffoldConfig = (): ClickHouseScaffoldConfig => ({
    host: host.trim(),
    port: parsedPort(),
    database: database.trim(),
    username: username.trim() || undefined,
    passwordVar: secretName.trim(),
    secure,
    tables: Object.entries(selection).map(([name, sel]) => ({
      name,
      writeDisposition: sel.disposition,
      cursorField: sel.disposition === "append" ? sel.cursorField || undefined : undefined,
      primaryKey: sel.disposition === "merge" && sel.primaryKey ? [sel.primaryKey] : undefined
    }))
  });

  /** Persisted as a secret; the YAML only references `password_var`. */
  const persistSecret = async (pipelineName: string) => {
    if (!password) return;
    await createSecret.mutateAsync({
      name: secretName.trim(),
      value: password,
      description: `ClickHouse password for pipeline ${pipelineName}`
    });
  };

  return {
    host,
    setHost,
    port,
    setPort,
    database,
    setDatabase,
    username,
    setUsername,
    password,
    setPassword,
    secretName,
    setSecretName,
    secure,
    setSecure,
    tables,
    selection,
    discovering: discoverTables.isPending,
    fetchTables,
    toggleTable,
    setTableConfig,
    selectedCount,
    allTablesSelected,
    toggleSelectAll,
    reset,
    validate,
    buildScaffoldConfig,
    persistSecret
  };
}
