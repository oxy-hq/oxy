import { ArrowLeft, ArrowRight } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Switch } from "@/components/ui/shadcn/switch";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { useDiscoverSourceTables } from "@/hooks/api/airway/useAirway";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useCreateFile from "@/hooks/api/files/useCreateFile";
import useSaveFile from "@/hooks/api/files/useSaveFile";
import { useCreateSecret } from "@/hooks/api/secrets/useSecretMutations";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import type { DiscoveredTable } from "@/services/api/airway";
import useCurrentOrg from "@/stores/useCurrentOrg";
import {
  buildPipelineScaffold,
  SOURCE_OPTIONS,
  WRITABLE_DESTINATION_DB_TYPES,
  type WriteDisposition
} from "../scaffold";
import ConnectorCard from "./ConnectorCard";

const WRITABLE = new Set<string>(WRITABLE_DESTINATION_DB_TYPES);

/** Per-table load config the user sets in the picker. */
interface TableSel {
  disposition: WriteDisposition;
  /** High-water-mark column for incremental `append`. */
  cursorField?: string;
  /** Upsert key for `merge`. */
  primaryKey?: string;
}

const DISPOSITIONS: { value: WriteDisposition; label: string; hint: string }[] = [
  {
    value: "append",
    label: "Append",
    hint: "insert rows (rerun duplicates unless a cursor is set)"
  },
  { value: "replace", label: "Replace", hint: "overwrite the whole table each run" },
  { value: "merge", label: "Merge", hint: "upsert on a key (idempotent reruns)" }
];

interface NewPipelineDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Existing pipeline display-names, for the duplicate-name guard. */
  existingNames: string[];
  /** Called after the file is created so the list can refresh. */
  onCreated: () => void;
}

type Step = 0 | 1 | 2;
const STEP_LABELS = ["Source", "Destination", "Details"] as const;

/** Show the per-step search box once a card grid gets this big. */
const SEARCH_THRESHOLD = 6;

const NAME_RE = /^[a-zA-Z0-9_-]+$/;

const NewPipelineDialog: React.FC<NewPipelineDialogProps> = ({
  open,
  onOpenChange,
  existingNames,
  onCreated
}) => {
  const [step, setStep] = useState<Step>(0);
  const [query, setQuery] = useState("");
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [destinationDb, setDestinationDb] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  // Toast source fields (only used when sourceId === "toast").
  const [toastClientId, setToastClientId] = useState("");
  const [toastClientSecret, setToastClientSecret] = useState("");
  const [toastSecretName, setToastSecretName] = useState("");
  const [toastGuids, setToastGuids] = useState("");
  const [toastBaseUrl, setToastBaseUrl] = useState("");

  // ClickHouse source fields (only used when sourceId === "clickhouse").
  const [chHost, setChHost] = useState("");
  const [chPort, setChPort] = useState("");
  const [chDatabase, setChDatabase] = useState("default");
  const [chUsername, setChUsername] = useState("default");
  const [chPassword, setChPassword] = useState("");
  const [chSecretName, setChSecretName] = useState("");
  const [chSecure, setChSecure] = useState(true);
  // Tables returned by discovery (with columns), and per-table load config
  // keyed by table name (presence == selected).
  const [chTables, setChTables] = useState<DiscoveredTable[] | null>(null);
  const [chSel, setChSel] = useState<Record<string, TableSel>>({});
  const discoverTables = useDiscoverSourceTables();

  // Clear the search when moving between steps so a stale query
  // doesn't hide the next step's cards.
  const goStep = (next: Step) => {
    setQuery("");
    setStep(next);
  };

  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const createFile = useCreateFile();
  const saveFile = useSaveFile();
  const createSecret = useCreateSecret();
  const navigate = useNavigate();
  const { data: databases, isLoading: databasesLoading } = useDatabases();
  const writableDatabases = (databases ?? []).filter((d) => WRITABLE.has(d.db_type));

  const reset = () => {
    setStep(0);
    setQuery("");
    setSourceId(null);
    setDestinationDb(null);
    setName("");
    setDescription("");
    setError(null);
    setToastClientId("");
    setToastClientSecret("");
    setToastSecretName("");
    setToastGuids("");
    setToastBaseUrl("");
    setChHost("");
    setChPort("");
    setChDatabase("default");
    setChUsername("default");
    setChPassword("");
    setChSecretName("");
    setChSecure(true);
    setChTables(null);
    setChSel({});
  };

  const parseGuids = (raw: string): string[] =>
    raw
      .split(/[\s,]+/)
      .map((g) => g.trim())
      .filter(Boolean);

  const validate = (): boolean => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Name is required");
      return false;
    }
    if (!NAME_RE.test(trimmed)) {
      setError("Use only letters, numbers, hyphens and underscores");
      return false;
    }
    if (existingNames.includes(trimmed)) {
      setError("A pipeline with this name already exists");
      return false;
    }
    setError(null);
    return true;
  };

  const parsedPort = (): number | undefined => {
    const p = chPort.trim();
    if (!p) return undefined;
    const n = Number(p);
    return Number.isInteger(n) && n > 0 ? n : undefined;
  };

  /** Live credentials for discovery + scaffold. Password included only
   *  when typed (discovery needs it; reuse-existing-secret can't list). */
  const chConfig = (): Record<string, unknown> => {
    const config: Record<string, unknown> = {
      host: chHost.trim(),
      database: chDatabase.trim(),
      username: chUsername.trim() || "default",
      secure: chSecure
    };
    const port = parsedPort();
    if (port != null) config.port = port;
    if (chPassword) config.password = chPassword;
    return config;
  };

  const handleFetchTables = async () => {
    if (!chHost.trim() || !chDatabase.trim()) {
      setError("ClickHouse: host and database are required to list tables");
      return;
    }
    setError(null);
    try {
      const tables = await discoverTables.mutateAsync({
        kind: "clickhouse",
        config: chConfig()
      });
      setChTables(tables);
      setChSel({});
    } catch (err) {
      toast.error("Couldn't list tables", {
        description: err instanceof Error ? err.message : "Check the credentials and try again."
      });
    }
  };

  const toggleTable = (name: string) => {
    setChSel((prev) => {
      const next = { ...prev };
      if (name in next) delete next[name];
      else next[name] = { disposition: "append" };
      return next;
    });
  };

  const setTable = (name: string, patch: Partial<TableSel>) => {
    setChSel((prev) => (name in prev ? { ...prev, [name]: { ...prev[name], ...patch } } : prev));
  };

  const selectedCount = Object.keys(chSel).length;
  const allTablesSelected =
    chTables != null && chTables.length > 0 && selectedCount === chTables.length;

  const toggleSelectAll = () => {
    if (!chTables) return;
    setChSel((prev) => {
      if (allTablesSelected) return {};
      const next = { ...prev };
      for (const t of chTables) if (!(t.name in next)) next[t.name] = { disposition: "append" };
      return next;
    });
  };

  const handleCreate = async () => {
    if (!sourceId || !destinationDb || !validate()) return;

    const isToast = sourceId === "toast";
    const isClickhouse = sourceId === "clickhouse";
    const guids = parseGuids(toastGuids);
    const secretName = toastSecretName.trim();
    const chSecret = chSecretName.trim();
    if (isToast) {
      if (!toastClientId.trim()) {
        setError("Toast: Client ID is required");
        return;
      }
      if (!secretName) {
        setError("Toast: a secret name for the client secret is required");
        return;
      }
      if (guids.length === 0) {
        setError("Toast: at least one restaurant GUID is required");
        return;
      }
    }
    if (isClickhouse) {
      if (!chHost.trim() || !chDatabase.trim()) {
        setError("ClickHouse: host and database are required");
        return;
      }
      if (!chSecret) {
        setError("ClickHouse: a secret name for the password is required");
        return;
      }
      if (!chPassword) {
        // Discovery uses the live password; clearing it before Create would
        // leave a `password_var` reference with no secret behind it.
        setError("ClickHouse: password is required");
        return;
      }
      if (selectedCount === 0) {
        setError("ClickHouse: select at least one table to ingest");
        return;
      }
      const mergeNoKey = Object.entries(chSel).find(
        ([, sel]) => sel.disposition === "merge" && !sel.primaryKey
      );
      if (mergeNoKey) {
        setError(`ClickHouse: pick a merge key for "${mergeNoKey[0]}" (or change its load mode)`);
        return;
      }
    }

    setCreating(true);
    try {
      const trimmed = name.trim();

      // Store the client secret in the secret manager. A blank value
      // means "reuse an existing secret with this name" — skip create.
      if (isToast && toastClientSecret.trim()) {
        await createSecret.mutateAsync({
          name: secretName,
          value: toastClientSecret,
          description: `Toast OAuth2 client secret for pipeline ${trimmed}`
        });
      }
      // Same for the ClickHouse password — persisted as a secret; the
      // YAML only references `password_var`.
      if (isClickhouse && chPassword) {
        await createSecret.mutateAsync({
          name: chSecret,
          value: chPassword,
          description: `ClickHouse password for pipeline ${trimmed}`
        });
      }

      const path = `pipelines/${trimmed}.airway.yml`;
      const pathb64 = encodeBase64(path);
      await createFile.mutateAsync(pathb64);
      await saveFile.mutateAsync({
        pathb64,
        data: buildPipelineScaffold({
          name: trimmed,
          description: description.trim() || undefined,
          sourceId,
          toast: isToast
            ? {
                clientId: toastClientId.trim(),
                clientSecretVar: secretName,
                restaurantGuids: guids,
                baseUrl: toastBaseUrl.trim() || undefined
              }
            : undefined,
          clickhouse: isClickhouse
            ? {
                host: chHost.trim(),
                port: parsedPort(),
                database: chDatabase.trim(),
                username: chUsername.trim() || undefined,
                passwordVar: chSecret,
                secure: chSecure,
                tables: Object.entries(chSel).map(([name, sel]) => ({
                  name,
                  writeDisposition: sel.disposition,
                  cursorField:
                    sel.disposition === "append" ? sel.cursorField || undefined : undefined,
                  primaryKey:
                    sel.disposition === "merge" && sel.primaryKey ? [sel.primaryKey] : undefined
                }))
              }
            : undefined,
          destinationDatabase: destinationDb,
          datasetName: trimmed,
          destinationIsAirhouse: writableDatabases
            .find((d) => d.name === destinationDb)
            ?.db_type.startsWith("airhouse")
        })
      });
      onCreated();
      onOpenChange(false);
      reset();
      // Open the YAML editor so the user fills in credentials/endpoints.
      navigate(ROUTES.ORG(orgSlug).WORKSPACE(project.id).IDE.FILES.FILE(pathb64));
    } catch (err) {
      toast.error("Failed to create pipeline", {
        description: err instanceof Error ? err.message : "There was a problem creating the file."
      });
      console.error("create pipeline failed:", err);
    } finally {
      setCreating(false);
    }
  };

  const q = query.trim().toLowerCase();
  const filteredSources = SOURCE_OPTIONS.filter(
    (o) =>
      !q ||
      o.label.toLowerCase().includes(q) ||
      o.description.toLowerCase().includes(q) ||
      o.id.includes(q)
  );
  const filteredDbs = writableDatabases.filter(
    (d) => !q || d.name.toLowerCase().includes(q) || d.db_type.toLowerCase().includes(q)
  );

  const searchBox = (placeholder: string) => (
    <Input
      autoFocus
      value={query}
      onChange={(e) => setQuery(e.target.value)}
      placeholder={placeholder}
      className='mb-2'
    />
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogContent className='flex max-h-[85dvh] flex-col sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>New pipeline</DialogTitle>
          <DialogDescription>
            {step === 0 && "Where does the data come from?"}
            {step === 1 && "Which database should it land in?"}
            {step === 2 && "Name it — source credentials are filled in the YAML editor next."}
          </DialogDescription>
        </DialogHeader>

        <div className='flex items-center gap-2 py-1'>
          {STEP_LABELS.map((labelText, i) => (
            <div key={labelText} className='flex items-center gap-2'>
              <span
                className={cn(
                  "flex h-5 w-5 items-center justify-center rounded-full text-xs",
                  i === step
                    ? "bg-primary text-primary-foreground"
                    : i < step
                      ? "bg-primary/20 text-primary"
                      : "bg-muted text-muted-foreground"
                )}
              >
                {i + 1}
              </span>
              <span
                className={cn(
                  "text-xs",
                  i === step ? "font-medium text-foreground" : "text-muted-foreground"
                )}
              >
                {labelText}
              </span>
              {i < STEP_LABELS.length - 1 && <span className='text-muted-foreground'>·</span>}
            </div>
          ))}
        </div>

        <div className='-mx-1 min-h-0 flex-1 overflow-y-auto px-1 py-2'>
          {step === 0 && (
            <div>
              {SOURCE_OPTIONS.length > SEARCH_THRESHOLD && searchBox("Search sources…")}
              {filteredSources.length === 0 ? (
                <p className='py-6 text-center text-muted-foreground text-sm'>
                  No source matches “{query}”.
                </p>
              ) : (
                <div className='grid max-h-72 grid-cols-2 gap-2 overflow-y-auto'>
                  {filteredSources.map((opt) => (
                    <ConnectorCard
                      key={opt.id}
                      label={opt.label}
                      description={opt.description}
                      selected={sourceId === opt.id}
                      onSelect={() => {
                        setSourceId(opt.id);
                        goStep(1);
                      }}
                    />
                  ))}
                </div>
              )}
            </div>
          )}

          {step === 1 &&
            (databasesLoading ? (
              <p className='py-6 text-center text-muted-foreground text-sm'>Loading databases…</p>
            ) : writableDatabases.length === 0 ? (
              <p className='rounded-md border border-border bg-muted/40 px-3 py-4 text-center text-muted-foreground text-sm'>
                No airway-writable database in config.yml. Add a Postgres or Airhouse database in
                Settings, then create a pipeline.
              </p>
            ) : (
              <div>
                {writableDatabases.length > SEARCH_THRESHOLD &&
                  searchBox("Search databases by name or type…")}
                {filteredDbs.length === 0 ? (
                  <p className='py-6 text-center text-muted-foreground text-sm'>
                    No database matches “{query}”.
                  </p>
                ) : (
                  <div className='grid max-h-72 grid-cols-2 gap-2 overflow-y-auto'>
                    {filteredDbs.map((db) => (
                      <ConnectorCard
                        key={db.name}
                        label={db.name}
                        description={db.db_type}
                        selected={destinationDb === db.name}
                        onSelect={() => {
                          setDestinationDb(db.name);
                          goStep(2);
                        }}
                      />
                    ))}
                  </div>
                )}
              </div>
            ))}

          {step === 2 && (
            <div className='grid gap-4'>
              <div className='rounded-md border border-border bg-muted/40 px-3 py-2 text-muted-foreground text-xs'>
                {SOURCE_OPTIONS.find((s) => s.id === sourceId)?.label}
                <ArrowRight className='mx-2 inline h-3 w-3' />
                {destinationDb}
              </div>
              <div className='grid gap-2'>
                <Label htmlFor='pipeline-name'>Name</Label>
                <Input
                  id='pipeline-name'
                  autoFocus
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value);
                    setError(null);
                  }}
                  placeholder='shopify_raw'
                  className={error ? "border-destructive" : ""}
                />
              </div>
              <div className='grid gap-2'>
                <Label htmlFor='pipeline-description'>Description</Label>
                <Textarea
                  id='pipeline-description'
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder='Optional — what this pipeline ingests'
                  rows={2}
                />
              </div>

              {sourceId === "toast" && (
                <div className='grid gap-3 rounded-md border border-border p-3'>
                  <p className='font-medium text-sm'>Toast credentials</p>
                  <div className='grid gap-2'>
                    <Label htmlFor='toast-client-id'>Client ID</Label>
                    <Input
                      id='toast-client-id'
                      value={toastClientId}
                      onChange={(e) => {
                        setToastClientId(e.target.value);
                        setError(null);
                      }}
                      placeholder='Toast OAuth2 client id'
                    />
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='toast-client-secret'>Client secret</Label>
                    <Input
                      id='toast-client-secret'
                      type='password'
                      value={toastClientSecret}
                      onChange={(e) => setToastClientSecret(e.target.value)}
                      placeholder='Stored as a secret — leave blank to reuse an existing one'
                    />
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='toast-secret-name'>Secret name</Label>
                    <Input
                      id='toast-secret-name'
                      value={toastSecretName}
                      onChange={(e) => {
                        setToastSecretName(e.target.value);
                        setError(null);
                      }}
                      placeholder='TOAST_CLIENT_SECRET'
                    />
                    <p className='text-muted-foreground text-xs'>
                      The pipeline references this name; the executor resolves it from the secret
                      manager at run time. Reuse an existing secret by entering its name and leaving
                      the value blank.
                    </p>
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='toast-guids'>Restaurant GUID(s)</Label>
                    <Textarea
                      id='toast-guids'
                      value={toastGuids}
                      onChange={(e) => {
                        setToastGuids(e.target.value);
                        setError(null);
                      }}
                      placeholder='One per line (or comma-separated)'
                      rows={2}
                    />
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='toast-base-url'>Base URL</Label>
                    <Input
                      id='toast-base-url'
                      value={toastBaseUrl}
                      onChange={(e) => setToastBaseUrl(e.target.value)}
                      placeholder='Optional — sandbox override (defaults to Toast prod)'
                    />
                  </div>
                </div>
              )}

              {sourceId === "clickhouse" && (
                <div className='grid gap-3 rounded-md border border-border p-3'>
                  <p className='font-medium text-sm'>ClickHouse connection</p>
                  <div className='grid grid-cols-[1fr_auto] gap-2'>
                    <div className='grid gap-2'>
                      <Label htmlFor='ch-host'>Host</Label>
                      <Input
                        id='ch-host'
                        value={chHost}
                        onChange={(e) => {
                          setChHost(e.target.value);
                          setError(null);
                        }}
                        placeholder='my-host.clickhouse.cloud'
                      />
                    </div>
                    <div className='grid w-24 gap-2'>
                      <Label htmlFor='ch-port'>Port</Label>
                      <Input
                        id='ch-port'
                        value={chPort}
                        onChange={(e) => setChPort(e.target.value)}
                        placeholder='8443'
                        inputMode='numeric'
                      />
                    </div>
                  </div>
                  <div className='grid grid-cols-2 gap-2'>
                    <div className='grid gap-2'>
                      <Label htmlFor='ch-database'>Database</Label>
                      <Input
                        id='ch-database'
                        value={chDatabase}
                        onChange={(e) => {
                          setChDatabase(e.target.value);
                          setError(null);
                        }}
                        placeholder='default'
                      />
                    </div>
                    <div className='grid gap-2'>
                      <Label htmlFor='ch-username'>Username</Label>
                      <Input
                        id='ch-username'
                        value={chUsername}
                        onChange={(e) => setChUsername(e.target.value)}
                        placeholder='default'
                      />
                    </div>
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='ch-password'>Password</Label>
                    <Input
                      id='ch-password'
                      type='password'
                      value={chPassword}
                      onChange={(e) => setChPassword(e.target.value)}
                      placeholder='Stored as a secret — needed to list tables'
                    />
                  </div>
                  <div className='grid gap-2'>
                    <Label htmlFor='ch-secret-name'>Secret name</Label>
                    <Input
                      id='ch-secret-name'
                      value={chSecretName}
                      onChange={(e) => {
                        setChSecretName(e.target.value);
                        setError(null);
                      }}
                      placeholder='CLICKHOUSE_PASSWORD'
                    />
                    <p className='text-muted-foreground text-xs'>
                      The pipeline references this name; the executor resolves it from the secret
                      manager at run time.
                    </p>
                  </div>
                  <div className='flex items-center justify-between'>
                    <Label htmlFor='ch-secure'>Use TLS (HTTPS)</Label>
                    <Switch id='ch-secure' checked={chSecure} onCheckedChange={setChSecure} />
                  </div>

                  <div className='flex items-center justify-between gap-2'>
                    <Button
                      type='button'
                      variant='secondary'
                      size='sm'
                      onClick={handleFetchTables}
                      disabled={
                        discoverTables.isPending ||
                        !chHost.trim() ||
                        !chDatabase.trim() ||
                        !chPassword
                      }
                    >
                      {discoverTables.isPending ? "Listing tables…" : "Fetch tables"}
                    </Button>
                    {chTables != null && chTables.length > 0 && (
                      <div className='flex items-center gap-2'>
                        <span className='text-muted-foreground text-xs'>
                          {selectedCount} of {chTables.length} selected
                        </span>
                        <Button type='button' variant='ghost' size='sm' onClick={toggleSelectAll}>
                          {allTablesSelected ? "Clear" : "Select all"}
                        </Button>
                      </div>
                    )}
                  </div>

                  {chTables != null &&
                    (chTables.length === 0 ? (
                      <p className='text-muted-foreground text-xs'>
                        No tables found in “{chDatabase.trim()}”.
                      </p>
                    ) : (
                      <div className='max-h-56 overflow-y-auto rounded-md border border-border'>
                        {chTables.map((t) => {
                          const sel = chSel[t.name];
                          return (
                            <div key={t.name} className='border-border border-b last:border-0'>
                              <label
                                htmlFor={`ch-table-${t.name}`}
                                className='flex cursor-pointer items-center gap-2 px-2 py-1.5 text-sm hover:bg-muted/50'
                              >
                                <Checkbox
                                  id={`ch-table-${t.name}`}
                                  checked={sel != null}
                                  onCheckedChange={() => toggleTable(t.name)}
                                />
                                <span className='truncate'>{t.name}</span>
                              </label>
                              {sel != null && (
                                <div className='flex flex-wrap items-center gap-2 px-2 pb-2 pl-8'>
                                  <Select
                                    value={sel.disposition}
                                    onValueChange={(v) =>
                                      setTable(t.name, { disposition: v as WriteDisposition })
                                    }
                                  >
                                    <SelectTrigger className='h-7 w-28 text-xs'>
                                      <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                      {DISPOSITIONS.map((d) => (
                                        <SelectItem key={d.value} value={d.value} title={d.hint}>
                                          {d.label}
                                        </SelectItem>
                                      ))}
                                    </SelectContent>
                                  </Select>

                                  {sel.disposition === "append" && (
                                    <Select
                                      value={sel.cursorField ?? "__none"}
                                      onValueChange={(v) =>
                                        setTable(t.name, {
                                          cursorField: v === "__none" ? undefined : v
                                        })
                                      }
                                    >
                                      <SelectTrigger className='h-7 w-44 text-xs'>
                                        <SelectValue />
                                      </SelectTrigger>
                                      <SelectContent>
                                        <SelectItem value='__none'>
                                          No cursor (full reload)
                                        </SelectItem>
                                        {t.columns.map((c) => (
                                          <SelectItem key={c.name} value={c.name}>
                                            {c.name}
                                          </SelectItem>
                                        ))}
                                      </SelectContent>
                                    </Select>
                                  )}

                                  {sel.disposition === "merge" && (
                                    <Select
                                      value={sel.primaryKey ?? ""}
                                      onValueChange={(v) => setTable(t.name, { primaryKey: v })}
                                    >
                                      <SelectTrigger className='h-7 w-44 text-xs'>
                                        <SelectValue placeholder='merge key (required)' />
                                      </SelectTrigger>
                                      <SelectContent>
                                        {t.columns.map((c) => (
                                          <SelectItem key={c.name} value={c.name}>
                                            {c.name}
                                          </SelectItem>
                                        ))}
                                      </SelectContent>
                                    </Select>
                                  )}
                                </div>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    ))}
                </div>
              )}

              {error && <p className='text-destructive text-sm'>{error}</p>}
            </div>
          )}
        </div>

        <DialogFooter className='sm:justify-between'>
          {step > 0 ? (
            <Button variant='ghost' onClick={() => goStep((step - 1) as Step)} disabled={creating}>
              <ArrowLeft className='h-4 w-4' />
              Back
            </Button>
          ) : (
            <Button variant='ghost' onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
          )}
          {step === 2 && (
            <Button onClick={handleCreate} disabled={creating || !name.trim()}>
              {creating ? "Creating..." : "Create"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default NewPipelineDialog;
