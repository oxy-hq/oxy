// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { AxiosError, type AxiosResponse } from "axios";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type AppEnvironment, CustomAppsService } from "@/services/api/customApps";
import type { AppSecrets } from "@/types/apps";

vi.mock("@/services/api/customApps", () => ({
  CustomAppsService: { listSecrets: vi.fn(), setSecret: vi.fn(), listEnvironments: vi.fn() }
}));

import { Secrets } from "./index";

const secrets: AppSecrets = {
  app_id: "app-1",
  app_slug: "storefront",
  app_name: "Storefront",
  environment: "production",
  missing_required: 1,
  entries: [
    { key: "STRIPE_KEY", is_set: false, declared: true, required: true, source: "manifest" },
    {
      key: "WEBHOOK_SECRET",
      is_set: true,
      declared: true,
      required: true,
      source: "webhook",
      secret_id: "s-1",
      updated_at: "2026-10-01T00:00:00Z"
    }
  ]
};

const environment = (name: string, kind: AppEnvironment["kind"]): AppEnvironment => ({
  name,
  kind,
  status: "active",
  build_id: null,
  build_uuid: null,
  semantic_revision_id: null,
  owner: null,
  created_at: "2026-10-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
  last_activity_at: null,
  expires_at: null,
  url: null,
  oltp_schema: null
});

beforeEach(() => {
  vi.mocked(CustomAppsService.listEnvironments).mockResolvedValue([
    environment("production", "production"),
    environment("staging", "staging"),
    environment("dev-luong", "dev")
  ]);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/** What the server answers a viewer oxy-authz does not let open non-production. */
const forbidden = () =>
  new AxiosError("Request failed with status code 403", "ERR_BAD_REQUEST", undefined, undefined, {
    status: 403,
    data: "staging secrets are set by Oxy staff who may open this app's non-production environments"
  } as AxiosResponse);

const renderPane = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <Secrets appId='app-1' />
    </QueryClientProvider>
  );

/** Renders the panel, then opens the dialog from `key`'s row button. */
const openFromRow = async (key: string, button: "Set" | "Rotate") => {
  vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(secrets);
  renderPane();
  const row = await screen.findByTestId(`admin-app-secret-${key}`);
  fireEvent.click(within(row).getByRole("button", { name: button }));
  return screen.getByRole("dialog");
};

// The row knows whether its key has a value; the dialog used to assume any key
// opened from a row had one, and offered to "rotate" a secret that was missing.
describe("Secrets panel — the dialog says what the row is", () => {
  it("sets a missing key, rather than rotating it", async () => {
    const dialog = await openFromRow("STRIPE_KEY", "Set");
    expect(within(dialog).getByRole("heading", { name: "Set STRIPE_KEY" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Rotate" })).not.toBeInTheDocument();
    // Still the declared name: typing another would not satisfy the declaration.
    expect(within(dialog).getByLabelText("Key")).toBeDisabled();
  });

  it("rotates a key that has a value", async () => {
    const dialog = await openFromRow("WEBHOOK_SECRET", "Rotate");
    expect(
      within(dialog).getByRole("heading", { name: "Rotate WEBHOOK_SECRET" })
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Rotate" })).toBeInTheDocument();
  });

  it("adds a new key from Add secret", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(secrets);
    render(
      <QueryClientProvider client={new QueryClient()}>
        <Secrets appId='app-1' />
      </QueryClientProvider>
    );
    fireEvent.click(screen.getByTestId("admin-app-secrets-add"));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Add secret" })).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Key")).toBeEnabled();
  });
});

describe("Secrets panel — environments", () => {
  it("lists production's secrets by default", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(secrets);
    renderPane();
    await screen.findByTestId("admin-app-secret-STRIPE_KEY");
    expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "production");
    expect(screen.getByTestId("admin-app-secrets-environment")).toHaveValue("production");
  });

  it("offers staging and each sandbox, and lists the one picked", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(secrets);
    renderPane();
    const picker = screen.getByTestId("admin-app-secrets-environment");
    await screen.findByRole("option", { name: "Sandbox dev-luong" });
    expect(
      within(picker)
        .getAllByRole("option")
        .map((o) => o.getAttribute("value"))
    ).toEqual(["production", "staging", "dev-luong"]);
    fireEvent.change(picker, { target: { value: "dev-luong" } });
    await waitFor(() =>
      expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "dev-luong")
    );
  });

  it("shows an inherited key as using the other environment's value, not as missing", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockImplementation(async (_id, env) =>
      env === "dev-luong"
        ? {
            ...secrets,
            environment: "dev-luong",
            missing_required: 0,
            entries: [
              {
                // Production holds it (`secrets` above).
                key: "WEBHOOK_SECRET",
                is_set: false,
                declared: true,
                required: true,
                source: "manifest",
                shared: true,
                inherits_production: true
              },
              {
                key: "QB_TOKEN",
                is_set: false,
                declared: true,
                required: true,
                source: "manifest",
                inherits_staging: true
              }
            ]
          }
        : secrets
    );
    renderPane();
    await screen.findByRole("option", { name: "Sandbox dev-luong" });
    fireEvent.change(screen.getByTestId("admin-app-secrets-environment"), {
      target: { value: "dev-luong" }
    });
    const shared = await screen.findByTestId("admin-app-secret-inherits-WEBHOOK_SECRET");
    expect(shared).toHaveTextContent("Uses production\u2019s value");
    expect(screen.getByTestId("admin-app-secret-inherits-QB_TOKEN")).toHaveTextContent(
      "Uses staging\u2019s value"
    );
    const row = screen.getByTestId("admin-app-secret-WEBHOOK_SECRET");
    expect(within(row).getByText("Shared")).toBeInTheDocument();
    expect(within(row).queryByText("Missing")).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("admin-app-secret-QB_TOKEN")).queryByText("Missing")
    ).not.toBeInTheDocument();
  });

  it("does not say a shared key uses production's value when production has none", async () => {
    // `inherits_production` says only that the key is shared and unset here; a
    // run reads nothing when production holds no value either (STRIPE_KEY is
    // unset in `secrets` above), so the row is missing, not inherited.
    vi.mocked(CustomAppsService.listSecrets).mockImplementation(async (_id, env) =>
      env === "dev-luong"
        ? {
            ...secrets,
            environment: "dev-luong",
            missing_required: 0,
            entries: [
              {
                key: "STRIPE_KEY",
                is_set: false,
                declared: true,
                required: true,
                source: "manifest",
                shared: true,
                inherits_production: true
              }
            ]
          }
        : secrets
    );
    renderPane();
    await screen.findByRole("option", { name: "Sandbox dev-luong" });
    fireEvent.change(screen.getByTestId("admin-app-secrets-environment"), {
      target: { value: "dev-luong" }
    });
    await waitFor(() =>
      expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "dev-luong")
    );
    const row = await screen.findByTestId("admin-app-secret-STRIPE_KEY");
    await waitFor(() => expect(within(row).getByText("Missing")).toBeInTheDocument());
    expect(screen.queryByTestId("admin-app-secret-inherits-STRIPE_KEY")).toBeNull();
  });

  it("falls back to production when the sandbox it shows is deleted", async () => {
    // A deleted sandbox's secrets answer 404 for good; retrying them behind
    // "Couldn't load", with Add secret still offered, helps nobody.
    vi.mocked(CustomAppsService.listSecrets).mockImplementation(async (_id, env) => {
      if (env === "dev-luong") return { ...secrets, environment: "dev-luong", entries: [] };
      return secrets;
    });
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <Secrets appId='app-1' />
      </QueryClientProvider>
    );
    await screen.findByRole("option", { name: "Sandbox dev-luong" });
    const picker = screen.getByTestId("admin-app-secrets-environment");
    fireEvent.change(picker, { target: { value: "dev-luong" } });
    await waitFor(() =>
      expect(CustomAppsService.listSecrets).toHaveBeenCalledWith("app-1", "dev-luong")
    );

    // Deleted from the Environments section: first torn down, then gone.
    vi.mocked(CustomAppsService.listEnvironments).mockResolvedValue([
      environment("production", "production"),
      environment("staging", "staging"),
      { ...environment("dev-luong", "dev"), status: "deleting" }
    ]);
    await client.invalidateQueries();
    await waitFor(() => expect(picker).toHaveValue("production"));
    expect(await screen.findByTestId("admin-app-secret-STRIPE_KEY")).toBeInTheDocument();
    expect(screen.queryByText(/Couldn.t load/)).not.toBeInTheDocument();
  });

  it("offers Copy from production only on a key production holds", async () => {
    // Production holds WEBHOOK_SECRET and lacks STRIPE_KEY (`secrets` above).
    const unset = (key: string) => ({
      key,
      is_set: false,
      declared: true,
      required: true,
      source: "manifest" as const
    });
    vi.mocked(CustomAppsService.listSecrets).mockImplementation(async (_id, env) =>
      env === "dev-luong"
        ? {
            ...secrets,
            environment: "dev-luong",
            entries: [unset("STRIPE_KEY"), unset("WEBHOOK_SECRET")]
          }
        : secrets
    );
    renderPane();
    await screen.findByRole("option", { name: "Sandbox dev-luong" });
    fireEvent.change(screen.getByTestId("admin-app-secrets-environment"), {
      target: { value: "dev-luong" }
    });
    const held = await screen.findByTestId("admin-app-secret-WEBHOOK_SECRET");
    expect(
      await within(held).findByRole("button", { name: "Copy from production" })
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("admin-app-secret-STRIPE_KEY")).queryByRole("button", {
        name: /Copy from/
      })
    ).toBeNull();
  });

  it("says plainly when the viewer may not open a non-production environment", async () => {
    vi.mocked(CustomAppsService.listSecrets).mockImplementation(async (_id, env) => {
      if (env === "staging") throw forbidden();
      return secrets;
    });
    renderPane();
    await screen.findByTestId("admin-app-secret-STRIPE_KEY");
    fireEvent.change(screen.getByTestId("admin-app-secrets-environment"), {
      target: { value: "staging" }
    });
    const refused = await screen.findByTestId("admin-app-secrets-nonprod-refused");
    expect(refused).toHaveTextContent("Staging secrets are set by Oxy staff");
    expect(screen.queryByText(/Couldn.t load/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("admin-app-secrets-add")).not.toBeInTheDocument();

    fireEvent.click(within(refused).getByRole("button", { name: "Back to production" }));
    expect(await screen.findByTestId("admin-app-secret-STRIPE_KEY")).toBeInTheDocument();
    expect(screen.getByTestId("admin-app-secrets-environment")).toHaveValue("production");
  });

  it("keeps production, and says why nothing else is offered, when environments are refused", async () => {
    vi.mocked(CustomAppsService.listEnvironments).mockRejectedValue(forbidden());
    vi.mocked(CustomAppsService.listSecrets).mockResolvedValue(secrets);
    renderPane();
    expect(await screen.findByTestId("admin-app-secrets-nonprod-locked")).toBeInTheDocument();
    const picker = screen.getByTestId("admin-app-secrets-environment");
    expect(
      within(picker)
        .getAllByRole("option")
        .map((o) => o.getAttribute("value"))
    ).toEqual(["production"]);
    expect(await screen.findByTestId("admin-app-secret-STRIPE_KEY")).toBeInTheDocument();
  });
});
