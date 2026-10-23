// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { AxiosError, type AxiosResponse } from "axios";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomAppsService } from "@/services/api/customApps";
import type { CustomDomain, CustomDomainsResponse } from "@/types/apps";

vi.mock("@/services/api/customApps", () => ({
  CustomAppsService: {
    listDomains: vi.fn(),
    addDomain: vi.fn(),
    verifyDomain: vi.fn(),
    removeDomain: vi.fn()
  }
}));

const toastError = vi.hoisted(() => vi.fn());
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: toastError } }));

import { CustomDomains } from "./index";

const domain = (over: Partial<CustomDomain>): CustomDomain => ({
  id: "d-1",
  domain: "app.acme.com",
  status: "pending",
  url: "https://app.acme.com/",
  txt_name: "_oxygen-challenge.app.acme.com",
  txt_value: "oxygen-verification=abc123",
  verified_at: null,
  last_checked_at: null,
  last_check_error: null,
  created_at: "2026-10-23T00:00:00Z",
  ...over
});

const respond = (domains: CustomDomain[], cname_target: string | null = "edge.oxy.tech") => {
  const body: CustomDomainsResponse = { domains, cname_target };
  vi.mocked(CustomAppsService.listDomains).mockResolvedValue(body);
};

const renderPane = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <CustomDomains appId='app-1' />
    </QueryClientProvider>
  );
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CustomDomains", () => {
  it("shows the two DNS records and Verify for a pending domain", async () => {
    respond([domain({ last_check_error: "TXT record not found" })]);
    renderPane();

    const row = await screen.findByTestId("admin-app-domain-row-d-1");
    expect(within(row).getByTestId("admin-app-domain-status-d-1").textContent).toBe("Pending");
    const cname = within(row).getByTestId("admin-app-domain-record-d-1-CNAME");
    expect(cname.textContent).toContain("app.acme.com");
    expect(cname.textContent).toContain("edge.oxy.tech");
    const txt = within(row).getByTestId("admin-app-domain-record-d-1-TXT");
    expect(txt.textContent).toContain("_oxygen-challenge.app.acme.com");
    expect(txt.textContent).toContain("oxygen-verification=abc123");
    expect(within(row).getByTestId("admin-app-domain-error-d-1").textContent).toContain(
      "TXT record not found"
    );
    expect(within(row).getByTestId("admin-app-domain-verify-d-1")).toBeTruthy();
  });

  it("asks for the routing target when the server has none", async () => {
    respond([domain({})], null);
    renderPane();
    expect((await screen.findByTestId("admin-app-domain-no-target-d-1")).textContent).toBe(
      "Ask Oxy ops for the routing target"
    );
  });

  it("links a verified domain and hides the records and Verify", async () => {
    respond([domain({ status: "verified", verified_at: "2026-10-23T01:00:00Z" })]);
    renderPane();

    const link = (await screen.findByTestId("admin-app-domain-link-d-1")) as HTMLAnchorElement;
    expect(link.href).toBe("https://app.acme.com/");
    expect(link.target).toBe("_blank");
    expect(screen.queryByTestId("admin-app-domain-verify-d-1")).toBeNull();
    expect(screen.queryByTestId("admin-app-domain-dns-d-1")).toBeNull();
  });

  it("adds a normalised domain", async () => {
    respond([]);
    vi.mocked(CustomAppsService.addDomain).mockResolvedValue(domain({}));
    renderPane();

    fireEvent.change(await screen.findByTestId("admin-app-domain-input"), {
      target: { value: " App.Acme.com " }
    });
    fireEvent.click(screen.getByTestId("admin-app-domain-add"));

    await waitFor(() =>
      expect(CustomAppsService.addDomain).toHaveBeenCalledWith("app-1", "app.acme.com")
    );
  });

  it("verifies, then refetches the list", async () => {
    respond([domain({})]);
    vi.mocked(CustomAppsService.verifyDomain).mockResolvedValue(domain({}));
    renderPane();

    fireEvent.click(await screen.findByTestId("admin-app-domain-verify-d-1"));

    await waitFor(() =>
      expect(CustomAppsService.verifyDomain).toHaveBeenCalledWith("app-1", "d-1")
    );
    await waitFor(() => expect(CustomAppsService.listDomains).toHaveBeenCalledTimes(2));
  });

  it("shows the server's message when verify is refused", async () => {
    respond([domain({})]);
    vi.mocked(CustomAppsService.verifyDomain).mockRejectedValue(
      new AxiosError(
        "Request failed with status code 409",
        "ERR_BAD_REQUEST",
        undefined,
        undefined,
        {
          status: 409,
          data: { error: "domain_taken", message: "Another app already verified this domain" }
        } as AxiosResponse
      )
    );
    renderPane();

    fireEvent.click(await screen.findByTestId("admin-app-domain-verify-d-1"));

    await waitFor(() =>
      expect(toastError).toHaveBeenCalledWith("Another app already verified this domain")
    );
  });

  it("removes only after the confirm", async () => {
    respond([domain({ status: "verified" })]);
    vi.mocked(CustomAppsService.removeDomain).mockResolvedValue(undefined);
    renderPane();

    fireEvent.click(await screen.findByTestId("admin-app-domain-remove-d-1"));
    expect(CustomAppsService.removeDomain).not.toHaveBeenCalled();

    fireEvent.click(await screen.findByTestId("admin-app-domain-confirm-ok"));
    await waitFor(() =>
      expect(CustomAppsService.removeDomain).toHaveBeenCalledWith("app-1", "d-1")
    );
  });
});
