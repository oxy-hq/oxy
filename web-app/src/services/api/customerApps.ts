import type {
  AppBuildHistory,
  CreateAppRequest,
  CustomerApp,
  CustomerAppDebug,
  ListdirResponse,
  OxyAccessGrant,
  ProbeResponse,
  Template,
  UpdateAppRequest
} from "@/types/apps";
import { apiClient } from "./axios";

/**
 * Customer-apps registry — gated by OXY_APP_ADMINS on the server. CRUD is
 * uuid-keyed (internal callers always know the uuid); sync uses the pretty
 * `<org_slug>/<app_slug>` path because CI calls it without ever seeing the
 * uuid.
 */
export const CustomerAppsService = {
  /**
   * Paged admin list. The server orders rows by `updated_at` DESC so
   * page 0 is "what got touched most recently"; `next_offset` is null
   * when we've walked off the end. Callers without an explicit page
   * use the server default size (50) — fine for the first paint, and
   * the infinite-query hook walks back from there.
   */
  async list(
    options: { limit?: number; offset?: number } = {}
  ): Promise<{ items: CustomerApp[]; next_offset: number | null }> {
    const params = new URLSearchParams();
    if (options.limit != null) params.set("limit", String(options.limit));
    if (options.offset != null) params.set("offset", String(options.offset));
    const suffix = params.toString();
    const response = await apiClient.get(`/customer-apps${suffix ? `?${suffix}` : ""}`);
    return response.data;
  },

  async listMine(): Promise<CustomerApp[]> {
    const response = await apiClient.get("/apps/mine");
    return response.data;
  },

  /**
   * Every workspace that granted Oxy access, platform-wide, for the admin
   * org/project browser. App-admin gated (same surface as the registry).
   */
  async listOxyAccess(): Promise<OxyAccessGrant[]> {
    const response = await apiClient.get("/customer-apps/oxy-access");
    return response.data;
  },

  async create(req: CreateAppRequest): Promise<CustomerApp> {
    const response = await apiClient.post("/customer-apps", req);
    return response.data;
  },

  async update(id: string, req: UpdateAppRequest): Promise<CustomerApp> {
    const response = await apiClient.patch(`/customer-apps/${id}`, req);
    return response.data;
  },

  async delete(id: string): Promise<void> {
    await apiClient.delete(`/customer-apps/${id}`);
  },

  /**
   * Publish the app. Until published, only Oxy staff (with workspace
   * oxy-access) can reach the URL; once published, it surfaces in
   * the owning workspace's sidebar.
   *
   * Hits the `/customer-apps/*` mount (app-admin gated) rather than
   * `/admin/apps/*` (OXY_OWNER gated) — publishing is a normal
   * Oxy-engineer workflow.
   */
  async publish(id: string): Promise<CustomerApp> {
    const response = await apiClient.post(`/customer-apps/${id}/publish`);
    return response.data;
  },

  async unpublish(id: string): Promise<CustomerApp> {
    const response = await apiClient.delete(`/customer-apps/${id}/publish`);
    return response.data;
  },

  /**
   * Versioned build history for the new publish pipeline, newest first.
   * Empty for legacy s3/local/v0 rows never published via `oxy publish`.
   */
  async listBuilds(id: string): Promise<AppBuildHistory> {
    const response = await apiClient.get(`/customer-apps/${id}/builds`);
    return response.data;
  },

  /**
   * Roll the published channel back to a retained build. Pure pointer
   * move server-side — the build's bytes already live in S3.
   */
  async rollback(id: string, buildId: string): Promise<CustomerApp> {
    const response = await apiClient.post(`/customer-apps/${id}/rollback`, {
      build_id: buildId
    });
    return response.data;
  },

  /**
   * Flip this staff session into draft-preview mode. Sets an HttpOnly
   * cookie that the serve + data-products handlers read to route to
   * the draft channel. App-admin gated.
   */
  async enablePreviewDraft(): Promise<void> {
    await apiClient.post("/customer-apps/preview-draft");
  },

  async disablePreviewDraft(): Promise<void> {
    await apiClient.delete("/customer-apps/preview-draft");
  },

  /**
   * Diagnostic snapshot — what the server currently sees about the app's
   * bundle dir + resolved manifest + producer wiring. Powers the
   * Info tab in the admin detail pane.
   */
  async debug(app: Pick<CustomerApp, "slug" | "org_slug">): Promise<CustomerAppDebug> {
    const response = await apiClient.get(
      `/customer-apps/${encodeURIComponent(app.org_slug)}/${encodeURIComponent(app.slug)}/debug`
    );
    return response.data;
  },

  /**
   * Server-side folder picker. Local-mode only — returns 404 in cloud
   * and the dialog hides the picker on that surface. Pass an absolute
   * path or an empty string for the server-chosen default landing
   * (`$OXY_STATE_DIR/customer-apps` if present, else `$HOME`).
   */
  async listdir(path: string, showHidden = false): Promise<ListdirResponse> {
    const params = new URLSearchParams();
    if (path) params.set("path", path);
    if (showHidden) params.set("show_hidden", "true");
    const response = await apiClient.get(`/customer-apps/fs/listdir?${params.toString()}`);
    return response.data;
  },

  /**
   * Bundle identity probe — reads `oxy-app.json` + `index.html` from
   * the picked folder so the dialog can lock the slug to whatever the
   * bundle declares. Only available in local mode; returns 404 in cloud.
   */
  async probe(path: string): Promise<ProbeResponse> {
    const params = new URLSearchParams({ path });
    const response = await apiClient.get(`/customer-apps/fs/probe?${params.toString()}`);
    return response.data;
  },

  /**
   * List the curated scaffold templates available on this server.
   * Templates are baked into the binary and never change at runtime.
   *
   * Defensive: validate the response is an array before returning.
   * The endpoint sits behind the SPA fallback in oxy's router, so a
   * routing regression / stale dev-server proxy could return
   * `index.html` (HTML string) instead of JSON. Without this check
   * the consumer's `.map()` would crash the entire dialog.
   */
  async listTemplates(): Promise<Template[]> {
    const { data } = await apiClient.get<unknown>("/customer-apps/templates");
    if (!Array.isArray(data)) {
      throw new Error(
        `Templates endpoint returned ${typeof data} instead of an array — ` +
          `is /customer-apps/templates falling through to the SPA fallback? ` +
          `Sample: ${JSON.stringify(data).slice(0, 80)}`
      );
    }
    return data as Template[];
  }
};
