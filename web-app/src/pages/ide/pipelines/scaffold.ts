/**
 * Pure `.airway.yml` scaffold builder.
 *
 * The "New Pipeline" wizard collects: name, description, a source, and
 * a destination *database* (referenced by name from the project's
 * `config.yml`). The open-ended source connector config is left as a
 * commented placeholder the user fills in the YAML editor; the
 * destination carries no credentials — the backend resolves the named
 * database into a connection string at run time (incl. per-user
 * `airhouse_managed` minting).
 *
 * A source *option* is what the user picks; `airwayKind` is what gets
 * written as the YAML `kind:` and must be one of the dispatch arms in
 * `agentic_airway::source_factory`. Vendor sources (e.g. Toast) are
 * `rest_api` under the hood with a vendor-flavoured config template.
 */

/** Connector kinds wired in `agentic_airway::source_factory`. */
type AirwaySourceKind = "rest_api" | "filesystem" | "sql_database" | "postgres_cdc" | "toast";

export interface SourceOption {
  /** Selection id — may differ from the airway kind (e.g. "toast"). */
  id: string;
  label: string;
  description: string;
  /** The `kind:` written to YAML (an airway source_factory arm). */
  airwayKind: AirwaySourceKind;
}

export const SOURCE_OPTIONS: SourceOption[] = [
  {
    id: "rest_api",
    label: "REST API",
    description: "Paginated JSON endpoints (most SaaS APIs)",
    airwayKind: "rest_api"
  },
  {
    id: "toast",
    label: "Toast POS",
    description: "Toast restaurant POS (orders, menus, labor)",
    airwayKind: "toast"
  },
  {
    id: "filesystem",
    label: "Filesystem",
    description: "Local or cloud object storage (S3, GCS, Azure)",
    airwayKind: "filesystem"
  },
  {
    id: "sql_database",
    label: "SQL Database",
    description: "Full / query-based extract from a relational DB",
    airwayKind: "sql_database"
  },
  {
    id: "postgres_cdc",
    label: "Postgres CDC",
    description: "Change-data-capture via logical replication",
    airwayKind: "postgres_cdc"
  }
];

/**
 * config.yml database types airway can write to. The backend resolver
 * maps these to an airway destination kind (postgres -> postgres,
 * airhouse / airhouse_managed -> airhouse).
 */
export const WRITABLE_DESTINATION_DB_TYPES = [
  "postgres",
  "redshift",
  "airhouse",
  "airhouse_managed"
] as const;

/** Per-option `config:` block, keyed by `SourceOption.id`. */
const SOURCE_CONFIG: Record<string, string> = {
  rest_api: `    base_url: https://api.example.com
    # Auth, pagination and per-endpoint options follow airway's
    # RestApiConfig. An empty list extracts nothing — add endpoints.
    endpoints: []`,
  // Fallback only — the wizard always supplies real Toast fields and
  // routes through `buildToastConfig`.
  toast: `    client_id: <toast-client-id>
    client_secret_var: TOAST_CLIENT_SECRET
    restaurant_guids:
      - "<restaurant-guid>"`,
  filesystem: `    base_path: /path/to/data # or s3://bucket/prefix, gs://..., az://...
    pattern: "*.jsonl"
    format: jsonl # json | jsonl | csv
    table_name: my_table`,
  sql_database: `    connection_string: postgresql://user:pass@host:5432/db
    backend: postgres # postgres | mysql | sqlite | mssql | oracle | custom
    tables:
      - name: my_table
        write_disposition: append # append | replace | merge`,
  postgres_cdc: `    connection_string: postgresql://user:pass@host:5432/db
    slot_name: oxy_slot
    publication_name: oxy_pub
    tables:
      - my_table
    initial_snapshot: true`
};

/** Toast wizard fields. `clientSecretVar` is the secret-manager name
 *  the executor resolves at run time — the secret itself is never
 *  written into the `.airway.yml`. */
interface ToastScaffold {
  clientId: string;
  clientSecretVar: string;
  restaurantGuids: string[];
  baseUrl?: string;
}

export interface ScaffoldInput {
  name: string;
  description?: string;
  /** A `SourceOption.id`. */
  sourceId: string;
  /** Required when `sourceId === "toast"`. */
  toast?: ToastScaffold;
  /** A `config.yml` database name (the resolved destination). */
  destinationDatabase: string;
  /** Logical dataset/schema written into the destination. */
  datasetName: string;
}

function buildToastConfig(t: ToastScaffold): string {
  const guids = (t.restaurantGuids.length ? t.restaurantGuids : ["<restaurant-guid>"])
    .map((g) => `      - "${g}"`)
    .join("\n");
  const lines = [
    `    client_id: ${t.clientId}`,
    `    client_secret_var: ${t.clientSecretVar}`,
    "    restaurant_guids:",
    guids
  ];
  if (t.baseUrl?.trim()) lines.push(`    base_url: ${t.baseUrl.trim()}`);
  return lines.join("\n");
}

/** Build the initial `.airway.yml` body for a freshly-created pipeline. */
export function buildPipelineScaffold(input: ScaffoldInput): string {
  const option = SOURCE_OPTIONS.find((o) => o.id === input.sourceId) ?? SOURCE_OPTIONS[0];
  const configBlock =
    option.id === "toast" && input.toast
      ? buildToastConfig(input.toast)
      : (SOURCE_CONFIG[option.id] ?? SOURCE_CONFIG[option.airwayKind]);
  const desc = input.description?.trim();
  const descLine = desc ? `description: ${JSON.stringify(desc)}\n` : "";
  return `name: ${input.name}
${descLine}source:
  kind: ${option.airwayKind}
  config:
${configBlock}
destination:
  # A database defined in config.yml. Credentials are resolved at run
  # time (airhouse_managed mints an ephemeral per-user credential).
  database: ${input.destinationDatabase}
  dataset_name: ${input.datasetName}

# Optional: restrict to a subset of the source's resources.
# resources: []

# In-flight extractions when running resources in parallel (1-16).
concurrency: 1

# Streaming (concurrent extract->sink, live per-table progress) is on
# by default for streaming-capable destinations. Uncomment to force
# the bulk path.
# streaming: false
`;
}
