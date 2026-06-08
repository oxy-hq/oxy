/**
 * Tagged source spec — matches the backend `SourceSpec` enum 1:1.
 * - `v0`: oxy auth-wraps a deployed Vercel app URL in an iframe (typically
 *   what v0 hands you when you publish, but any Vercel-hosted app works).
 *   The wire-tag stayed `v0` for back-compat; treat it as "external
 *   Vercel deployment". **Experimental** — the wrapped URL is reachable
 *   directly outside oxy today (no reverse-proxy yet), so don't put
 *   oxy-protected data behind it.
 * - `local`: oxy reads bundle files from `<path>/out/<asset>` on the host
 *   filesystem. Only sensible when oxy is running on the same machine as
 *   the registering admin.
 * - `s3`: bundle lives at `s3://<bucket>/apps/<org_slug>/<app_slug>/out/`,
 *   pulled into the state dir by `POST /api/customer-apps/<org>/<app>/sync`.
 */
export type CustomerAppSource =
  | { type: "v0"; url: string }
  | { type: "local"; path: string }
  | { type: "s3" };

export interface CustomerApp {
  id: string;
  /** URL slug, unique within org. Auto-derived from name on create. */
  slug: string;
  name: string;
  org_id: string;
  /** Denormalised from organizations.slug for sync URL construction. */
  org_slug: string;
  project_id: string;
  branch: string;
  source_repo: string;
  status: string;
  /**
   * Canonical pretty URL `<base>/customer-apps/<org_slug>/<app_slug>/`.
   * Always set; works for every source_type.
   */
  url: string;
  /**
   * Subdomain URL for v0 sources when this cluster has
   * `OXY_CUSTOMER_APPS_SUBDOMAIN_SUFFIX` configured, e.g.
   * `https://mars--command-center.customer-apps-dev.oxygen-hq.com/`.
   * `null` otherwise — admin UI shows whichever URLs are present.
   */
  url_subdomain: string | null;
  source_type: "v0" | "local" | "s3";
  source_config: Record<string, unknown>;
  /** PR URL set by the scaffold flow when `scaffold_pr: true` was passed. */
  bootstrap_pr_url: string | null;
  last_synced_at: string | null;
  last_deploy_at: string | null;
  /**
   * MAX(custom_app_view_event.viewed_at) — last time anyone opened
   * this app in a browser. NULL until the first view is recorded.
   * Populated on list responses (batched); detail responses leave it
   * absent because the Activity tab fetches a richer view.
   */
  last_active_at?: string | null;
  /**
   * Set when an Oxy engineer publishes the app via the admin UI. Null
   * = draft (only Oxy staff with workspace oxy-access can reach it).
   * Once set, the app appears in the customer's workspace sidebar.
   */
  published_at: string | null;
  /**
   * Stable bundle identifier in the customer-apps git repo
   * (`<repo-org>/<repo-slug>`). Drives the S3 key
   * (`customer-apps/<repo_path>/{draft,published}/...`). Null on
   * non-S3 sources; on S3 sources, defaults to `<org_slug>/<slug>`.
   */
  repo_path: string | null;
  created_at: string;
  updated_at: string;
  /**
   * Soft warnings emitted by the server after a create / update
   * mutation. Empty / absent on list + get responses. Surfaced as
   * toasts so operators catch misconfiguration before they preview.
   */
  warnings?: string[];
}

/**
 * Summary returned by `GET /api/{workspaceId}/custom-apps`. Lighter
 * than [`CustomerApp`] because the workspace sidebar only needs the
 * label + the URL it links to.
 */
export interface CustomAppSummary {
  id: string;
  slug: string;
  name: string;
  org_slug: string;
  url: string;
  published_at: string;
}

export interface Template {
  id: string;
  name: string;
  description: string;
}

/**
 * One row of an app's versioned build history (new publish pipeline).
 * `id` is the build's primary key — pass it to rollback. `is_draft` /
 * `is_published` mark which channel currently points at this build.
 */
export interface AppBuild {
  id: string;
  build_id: string;
  created_at: string;
  is_draft: boolean;
  is_published: boolean;
  /** Email of the app-admin who published this build. Null for builds
   * created before publisher tracking, or via legacy paths. */
  published_by_email?: string | null;
}

/**
 * `GET /{id}/builds` response: the build history plus who last promoted a
 * build to live (promote draft or Make Live/rollback) — distinct from each
 * build's original `published_by_email`.
 */
export interface AppBuildHistory {
  builds: AppBuild[];
  promoted_by_email?: string | null;
  promoted_at?: string | null;
}

export interface CreateAppRequest {
  name: string;
  org_id: string;
  project_id: string;
  branch?: string;
  /** Optional explicit slug. If absent, derived from `name` and deduped. */
  slug?: string;
  /** Defaults to `{ type: "s3" }` on the server when omitted. */
  source?: CustomerAppSource;
  /**
   * When true and `source.type === "s3"`, the backend opens a PR on
   * `OXY_CUSTOMER_APPS_REPO` scaffolding `apps/<org>/<slug>/` before
   * returning. The PR URL ends up on the response's `bootstrap_pr_url`.
   */
  scaffold_pr?: boolean;
  /**
   * When true and `source.type === "local"`, oxy creates
   * `$OXY_STATE_DIR/customer-apps/<id>/source/` and pre-populates
   * `source.path` with that path. Lets "Create new" skip the
   * folder-doesn't-exist-yet gotcha — operators get a guaranteed-good
   * path baked into the freshly-inserted row.
   */
  provision_local_source?: boolean;
  /** Curated template to scaffold from. Defaults to "vite" server-side. */
  template_id?: string;
  /**
   * Stable bundle identifier — the `<repo-org>/<repo-slug>` path
   * under the customer-apps git repo where this bundle's source
   * lives. Drives the S3 key
   * (`customer-apps/<repo_path>/{draft,published}/...`) so the bundle
   * has the same storage path across every environment.
   *
   * Only meaningful for `source.type === "s3"`. Defaults server-side
   * to the row's `<org_slug>/<slug>` pair when omitted — covers the
   * common case where the admin row's identity matches the repo
   * layout. Set explicitly when per-env slug drift would otherwise
   * put the same bundle at different S3 paths in dev vs prod.
   */
  repo_path?: string;
}

/**
 * One Oxy-access grant from `GET /api/customer-apps/oxy-access`: a workspace
 * whose org enabled "let Oxy build apps on our data", flattened with its org
 * + grant metadata. Powers the admin Orgs / Projects browser.
 */
export interface OxyAccessGrant {
  workspace_id: string;
  workspace_name: string;
  org_id: string;
  org_name: string;
  org_slug: string;
  granted_by_email: string | null;
  granted_at: string;
}

/**
 * Response shape for `GET /api/customer-apps/fs/listdir`. Local-mode
 * only (404 in cloud). Used by the create-app dialog's folder picker.
 */
export interface ListdirResponse {
  path: string;
  parent: string | null;
  entries: Array<{
    name: string;
    path: string;
    is_dir: boolean;
  }>;
}

/**
 * Response shape for `GET /api/customer-apps/fs/probe?path=<abs>`.
 * Reads `oxy-app.json` + `index.html` from the picked folder so the
 * dialog can lock the slug to what the bundle declares — overriding
 * a manifest slug produces a bundle that 404s every data fetch
 * (the baked base path won't match the route).
 */
export interface ProbeResponse {
  /** False when the manifest fails v2 validation. The dialog should
   *  surface `warnings` and block submission when this is false. */
  ok: boolean;
  /** Human-readable explanations for any validation failures. Empty
   *  when `ok` is true. */
  warnings: string[];
  bundle_dir: string;
  /** Display name declared in `oxy-app.json`. */
  manifest_name: string | null;
  /** Slug declared in `oxy-app.json`. When set, the dialog locks the
   *  slug field — this is the authoritative source. */
  manifest_slug: string | null;
  /** Org slug declared in `oxy-app.json`. Prefills the dialog's org
   *  picker; operator can still override. No access weight — the
   *  actual gate is on the linked row. */
  manifest_org_slug: string | null;
  /** Project (workspace) uuid declared in `oxy-app.json`. Prefills
   *  the dialog's project picker; operator can still override. */
  manifest_project_id: string | null;
  /** `/customer-apps/<org>/<slug>/` baked into the bundle's
   *  `index.html` at build time. When set and the chosen slug doesn't
   *  match, the bundle won't work — dashboard sits at "Loading…". */
  baked_base_path: string | null;
  /** True when the folder contains an `index.html` at any of the
   *  candidate roots (`<path>`, `<path>/out`, `<path>/dist`). */
  has_index_html: boolean;
  /** Whether the bundle's source uses `@oxy-hq/vite-plugin` (the Oxy
   *  App Kit). `null` when undetermined (no nearby `package.json`).
   *  `true` when the plugin is in dependencies / devDependencies.
   *  `false` when a package.json exists but the plugin isn't listed.
   *  The dialog uses this to surface a one-line nudge — not a
   *  blocking warning; many bundles stay hand-rolled. */
  uses_oxy_kit: boolean | null;
}

export interface UpdateAppRequest {
  name?: string;
  slug?: string;
  project_id?: string;
  branch?: string;
  status?: string;
  /**
   * Repoint the bundle source. Most useful for LocalFolder paths
   * (fixing a wrong-folder mistake) and for moving an app between
   * v0 / local / s3 without delete + recreate.
   */
  source?: CustomerAppSource;
}

/**
 * Diagnostic snapshot from `GET /api/customer-apps/<org>/<app>/debug`.
 * Loose by design — the admin UI inspects it for humans; field
 * additions on the server should not break clients.
 */
export interface CustomerAppDebug {
  org_slug: string;
  app_slug: string;
  app: {
    id: string;
    slug: string;
    name: string;
    status: string;
    source_type: "v0" | "local" | "s3";
  };
  bundle_dir: string | null;
  bundle_dir_exists: boolean;
  /**
   * Where the served manifest came from. `remote` means the bundle is
   * served by an external host (v0/Vercel) through the reverse proxy —
   * there's no oxy-side `oxy-app.json`, so `manifest` and `bundle_dir`
   * are intentionally absent.
   */
  manifest_source: "db_override" | "bundle_file" | "remote";
  /** Loose by design — server schema can grow without breaking clients. */
  manifest?: unknown;
  manifest_error: string | null;
  /** Upstream URL when `manifest_source = "remote"`; null otherwise. */
  upstream_url: string | null;
}
