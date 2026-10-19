// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AxiosError, type AxiosResponse } from "axios";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { initiateGoogleAuth } from "@/hooks/auth/useGoogleAuth";
import { initiateOktaAuth } from "@/hooks/auth/useOktaAuth";
import type { AuthConfigResponse } from "@/types/auth";
import MobileAuth from ".";
import type { MobileSession } from "./mobileSession";

const CHALLENGE = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const REDIRECT = "oxygen://auth";
const URL_OK = `/mobile-auth?code_challenge=${CHALLENGE}&redirect_uri=${encodeURIComponent(
  REDIRECT
)}&state=nonce`;

// Every navigation goes through these two, so a test can see where the page would go.
const leaveTo = vi.fn();
const returnToApp = vi.fn();
let session: MobileSession | null = { name: "Ada Lovelace", email: "ada@example.com" };
vi.mock("./mobileSession", () => ({
  leaveTo: (url: string) => leaveTo(url),
  returnToApp: (url: string) => returnToApp(url),
  readMobileSession: () => Promise.resolve(session)
}));

// Each provider's own start is the seam: it builds the provider URL and carries the return URL.
vi.mock("@/hooks/auth/useGoogleAuth", () => ({ initiateGoogleAuth: vi.fn(async () => {}) }));
vi.mock("@/hooks/auth/useOktaAuth", () => ({ initiateOktaAuth: vi.fn(async () => {}) }));

const ALL_PROVIDERS = {
  auth_enabled: true,
  mode: "cloud",
  magic_link: true,
  google: { client_id: "google-client" },
  okta: { client_id: "okta-client", domain: "acme.okta.com" },
  github: { client_id: "github-client" },
  observability_enabled: false
} as AuthConfigResponse;
let authConfig: AuthConfigResponse = ALL_PROVIDERS;
vi.mock("@/contexts/AuthContext", () => ({ useAuth: () => ({ authConfig }) }));

type MutateOptions = { onSuccess?: (data: { code: string }) => void; onError?: (e: Error) => void };
const mutate = vi.fn();
let mutation: { isPending: boolean; isError: boolean; error: Error | null } = {
  isPending: false,
  isError: false,
  error: null
};
vi.mock("@/hooks/api/mobileAuth/useAuthorizeMobile", () => ({
  default: () => ({ mutate, ...mutation })
}));

const refused = (status: number, code: string): AxiosError => {
  const error = new AxiosError("refused");
  error.response = { status, data: { error: "no", code } } as AxiosResponse;
  return error;
};

afterEach(() => {
  cleanup();
  leaveTo.mockReset();
  returnToApp.mockReset();
  mutate.mockReset();
  vi.mocked(initiateGoogleAuth).mockReset();
  vi.mocked(initiateOktaAuth).mockReset();
  authConfig = ALL_PROVIDERS;
  session = { name: "Ada Lovelace", email: "ada@example.com" };
  mutation = { isPending: false, isError: false, error: null };
});

const open = (url: string) =>
  render(
    <MemoryRouter initialEntries={[url]}>
      <MobileAuth />
    </MemoryRouter>
  );

const confirmLast = (outcome: { code?: string; error?: Error }) => {
  const options = mutate.mock.calls.at(-1)?.[1] as MutateOptions;
  if (outcome.code) options.onSuccess?.({ code: outcome.code });
  if (outcome.error) options.onError?.(outcome.error);
};

describe("/mobile-auth", () => {
  it("names the account and waits: no request and no redirect without a tap", async () => {
    open(URL_OK);
    expect(await screen.findByTestId("mobile-auth-account")).toHaveTextContent("Ada Lovelace");
    expect(screen.getByTestId("mobile-auth-card")).toHaveTextContent("ada@example.com");
    expect(mutate).not.toHaveBeenCalled();
    expect(returnToApp).not.toHaveBeenCalled();
    expect(leaveTo).not.toHaveBeenCalled();
  });

  it("falls back to the email when the account has no name", async () => {
    session = { email: "ada@example.com" };
    open(URL_OK);
    expect(await screen.findByTestId("mobile-auth-account")).toHaveTextContent("ada@example.com");
  });

  it("on Continue, authorizes and returns the code and state to the app", async () => {
    const user = userEvent.setup();
    open(URL_OK);
    await user.click(await screen.findByTestId("mobile-auth-continue"));
    expect(mutate).toHaveBeenCalledWith(
      { code_challenge: CHALLENGE, code_challenge_method: "S256", redirect_uri: REDIRECT },
      expect.anything()
    );
    confirmLast({ code: "one-time" });
    expect(returnToApp).toHaveBeenCalledWith("oxygen://auth?code=one-time&state=nonce");
    expect(await screen.findByText("Signed in")).toBeInTheDocument();
  });

  it("on Cancel, tells the app the person declined and authorizes nothing", async () => {
    const user = userEvent.setup();
    open(URL_OK);
    await user.click(await screen.findByTestId("mobile-auth-cancel"));
    expect(returnToApp).toHaveBeenCalledWith("oxygen://auth?error=access_denied&state=nonce");
    expect(mutate).not.toHaveBeenCalled();
    expect(await screen.findByText("Sign-in cancelled")).toBeInTheDocument();
  });

  it("sends a signed-out browser through login, back to this exact request", async () => {
    session = null;
    open(URL_OK);
    await waitFor(() => expect(leaveTo).toHaveBeenCalledTimes(1));
    const target = new URL(leaveTo.mock.calls[0][0], "http://localhost");
    expect(target.pathname).toBe("/login");
    const back = new URL(target.searchParams.get("return_to") ?? "");
    expect(back.pathname).toBe("/mobile-auth");
    expect(back.searchParams.get("code_challenge")).toBe(CHALLENGE);
    expect(back.searchParams.get("redirect_uri")).toBe(REDIRECT);
    expect(back.searchParams.get("state")).toBe("nonce");
  });

  it("goes straight to Google's sign-in when the app says the person picked Google", async () => {
    session = null;
    open(`${URL_OK}&provider=google`);
    await waitFor(() => expect(initiateGoogleAuth).toHaveBeenCalledTimes(1));
    const [clientId, returnTo] = vi.mocked(initiateGoogleAuth).mock.calls[0];
    expect(clientId).toBe("google-client");
    const back = new URL(returnTo ?? "");
    expect(back.origin).toBe(window.location.origin);
    expect(back.pathname).toBe("/mobile-auth");
    expect(back.searchParams.get("code_challenge")).toBe(CHALLENGE);
    expect(back.searchParams.get("redirect_uri")).toBe(REDIRECT);
    expect(back.searchParams.get("state")).toBe("nonce");
    // Back here still signed out, it stops at the login page rather than looping through Google.
    expect(back.searchParams.has("provider")).toBe(false);
    expect(leaveTo).not.toHaveBeenCalled();
  });

  it("goes straight to Okta's sign-in with the deployment's Okta domain", async () => {
    session = null;
    open(`${URL_OK}&provider=okta`);
    await waitFor(() => expect(initiateOktaAuth).toHaveBeenCalledTimes(1));
    const [clientId, domain, returnTo] = vi.mocked(initiateOktaAuth).mock.calls[0];
    expect([clientId, domain]).toEqual(["okta-client", "acme.okta.com"]);
    expect(new URL(returnTo ?? "").pathname).toBe("/mobile-auth");
    expect(leaveTo).not.toHaveBeenCalled();
  });

  it("opens the login page on its email field when the person picked email", async () => {
    session = null;
    open(`${URL_OK}&provider=email`);
    await waitFor(() => expect(leaveTo).toHaveBeenCalledTimes(1));
    const target = new URL(leaveTo.mock.calls[0][0], "http://localhost");
    expect(target.pathname).toBe("/login");
    expect(target.searchParams.get("focus")).toBe("email");
    expect(new URL(target.searchParams.get("return_to") ?? "").pathname).toBe("/mobile-auth");
  });

  it.each([
    ["an unknown provider", "&provider=myspace", ALL_PROVIDERS],
    // A popup no tap opened is blocked: the login page's GitHub button is the tap.
    ["GitHub", "&provider=github", ALL_PROVIDERS],
    [
      "Google on a deployment without it",
      "&provider=google",
      { ...ALL_PROVIDERS, google: undefined }
    ],
    ["email on a deployment without it", "&provider=email", { ...ALL_PROVIDERS, magic_link: false }]
  ])("sends %s to the login page, as before", async (_what, query, config) => {
    session = null;
    authConfig = config;
    open(`${URL_OK}${query}`);
    await waitFor(() => expect(leaveTo).toHaveBeenCalledTimes(1));
    const target = new URL(leaveTo.mock.calls[0][0], "http://localhost");
    expect(target.pathname).toBe("/login");
    expect(target.searchParams.has("focus")).toBe(false);
    expect(new URL(target.searchParams.get("return_to") ?? "").pathname).toBe("/mobile-auth");
    expect(initiateGoogleAuth).not.toHaveBeenCalled();
  });

  it("falls back to the login page when the provider's sign-in can't start", async () => {
    session = null;
    vi.mocked(initiateGoogleAuth).mockRejectedValueOnce(new Error("state endpoint down"));
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    open(`${URL_OK}&provider=google`);
    await waitFor(() => expect(leaveTo).toHaveBeenCalledTimes(1));
    expect(leaveTo.mock.calls[0][0]).toMatch(/^\/login\?return_to=/);
    quiet.mockRestore();
  });

  it("ignores the provider when already signed in, and asks as always", async () => {
    open(`${URL_OK}&provider=google`);
    expect(await screen.findByTestId("mobile-auth-account")).toHaveTextContent("Ada Lovelace");
    expect(initiateGoogleAuth).not.toHaveBeenCalled();
    expect(leaveTo).not.toHaveBeenCalled();
    expect(mutate).not.toHaveBeenCalled();
  });

  it("signs in again when the session lapsed while the page sat open", async () => {
    const user = userEvent.setup();
    open(URL_OK);
    await user.click(await screen.findByTestId("mobile-auth-continue"));
    confirmLast({ error: refused(401, "unauthorized") });
    expect(leaveTo).toHaveBeenCalledWith(expect.stringMatching(/^\/login\?return_to=/));
    expect(returnToApp).not.toHaveBeenCalled();
  });

  it("explains a redirect the server refuses, and sends nothing to it", async () => {
    const user = userEvent.setup();
    open(URL_OK);
    await user.click(await screen.findByTestId("mobile-auth-continue"));
    confirmLast({ error: refused(400, "invalid_redirect_uri") });
    expect(await screen.findByText("This sign-in link doesn't work")).toBeInTheDocument();
    expect(screen.queryByTestId("mobile-auth-continue")).not.toBeInTheDocument();
    expect(returnToApp).not.toHaveBeenCalled();
  });

  it("explains that a crew member's session cannot sign the app in", async () => {
    const user = userEvent.setup();
    open(URL_OK);
    await user.click(await screen.findByTestId("mobile-auth-continue"));
    confirmLast({ error: refused(403, "frontline_not_supported") });
    expect(await screen.findByText("Crew accounts can't use the Oxygen app")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-auth-card")).toHaveTextContent(
      "Crew sign-in works only on a store's enrolled device."
    );
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(returnToApp).not.toHaveBeenCalled();
    expect(leaveTo).not.toHaveBeenCalled();
  });

  it("shows other failures inline, keeping the choice open", async () => {
    mutation = { isPending: false, isError: true, error: refused(500, "internal") };
    open(URL_OK);
    expect(await screen.findByTestId("mobile-auth-error")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-auth-continue")).toBeEnabled();
  });

  it.each([
    ["a website", "https://evil.example.com/cb"],
    ["Expo Go", "exp://192.168.1.4:8081/--/auth"],
    ["a query on the redirect", "oxygen://auth?next=https://evil.example.com"]
  ])("refuses a redirect to %s without a button to press", async (_what, redirect) => {
    open(
      `/mobile-auth?code_challenge=${CHALLENGE}&redirect_uri=${encodeURIComponent(redirect)}&state=s`
    );
    expect(await screen.findByText("This sign-in link doesn't work")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(returnToApp).not.toHaveBeenCalled();
    expect(leaveTo).not.toHaveBeenCalled();
  });

  it("asks nothing and sends nothing inside another page", async () => {
    const top = vi.spyOn(window, "top", "get").mockReturnValue({} as Window);
    open(URL_OK);
    expect(await screen.findByText("Open this page in its own tab")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    top.mockRestore();
  });
});
