import { clearAllCookies, clearBrowserStorage, redirectToHome } from "./browser";

export interface LogoutResponse {
  logout_url?: string;
  success: boolean;
  message: string;
}

export const performLogoutCleanup = (): void => {
  clearBrowserStorage();
  clearAllCookies();
};

/**
 * Handles the complete logout process including API call and cleanup
 */
export const handleLogout = async (): Promise<void> => {
  try {
    const response = await fetch("/api/logout", {
      credentials: "include",
      method: "GET"
    });

    if (response.ok) {
      performLogoutCleanup();
      redirectToHome();
    }
  } catch (error) {
    console.error("Logout error:", error);
    performLogoutCleanup();
    redirectToHome();
  }
};
