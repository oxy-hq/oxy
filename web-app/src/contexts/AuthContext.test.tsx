// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { redirectToHome } from "@/libs/utils";
import { clearAuthScopedStorage } from "@/libs/utils/authStorage";
import { AuthService } from "@/services/api/auth";
import type { AuthConfigResponse } from "@/types/auth";
import { AuthProvider, useAuth } from "./AuthContext";

// The session cookie is HttpOnly, so the only way logout clears it is by
// hitting the backend. These tests guard the regression where `logout()` was
// purely client-side and left `oxy_session` intact — which kept custom-app
// subdomains loading after sign-out.
vi.mock("@/services/api/auth", () => ({
  AuthService: { logout: vi.fn().mockResolvedValue(undefined) }
}));
vi.mock("@/libs/utils", () => ({ redirectToHome: vi.fn() }));
vi.mock("@/libs/utils/authStorage", () => ({ clearAuthScopedStorage: vi.fn() }));
vi.mock("@/libs/utils/onboardingStorage", () => ({
  clearAllOnboardingState: vi.fn(),
  clearLegacyLocalOnboardingState: vi.fn()
}));

const authConfig = { mode: "cloud" } as AuthConfigResponse;

function LogoutProbe() {
  const { logout } = useAuth();
  return (
    <button type='button' data-testid='logout-btn' onClick={() => logout()}>
      logout
    </button>
  );
}

function renderWithLogout() {
  render(
    <AuthProvider authConfig={authConfig}>
      <LogoutProbe />
    </AuthProvider>
  );
}

describe("AuthContext.logout", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("calls the backend /logout before clearing the local session", async () => {
    renderWithLogout();
    fireEvent.click(screen.getByTestId("logout-btn"));

    await waitFor(() => expect(AuthService.logout).toHaveBeenCalledTimes(1));
    expect(clearAuthScopedStorage).toHaveBeenCalledTimes(1);
    expect(redirectToHome).toHaveBeenCalledTimes(1);

    // The server round-trip must happen while the bearer token is still in
    // localStorage, i.e. before local teardown runs.
    expect(vi.mocked(AuthService.logout).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(clearAuthScopedStorage).mock.invocationCallOrder[0]
    );
  });

  it("still clears the local session and redirects when backend logout fails", async () => {
    vi.mocked(AuthService.logout).mockRejectedValueOnce(new Error("network"));
    renderWithLogout();
    fireEvent.click(screen.getByTestId("logout-btn"));

    await waitFor(() => expect(clearAuthScopedStorage).toHaveBeenCalledTimes(1));
    expect(redirectToHome).toHaveBeenCalledTimes(1);
  });
});
