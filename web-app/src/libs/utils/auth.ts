import {
  clearAllCookies,
  clearBrowserStorage,
  redirectToHome,
} from "./browser";

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
      method: "GET",
    });

    if (response.ok) {
      const logoutData: LogoutResponse = await response.json();

      // Clear local storage and cookies regardless of auth mode
      performLogoutCleanup();

      // If there's a logout URL (e.g., for aws Cognito), redirect to it
      if (logoutData.logout_url) {
        window.location.href = logoutData.logout_url;
      } else {
        // For other auth modes (IAP, Local), redirect to home page
        redirectToHome();
      }
    } else {
      // Fallback: clear everything and redirect to home
      performLogoutCleanup();
      redirectToHome();
    }
  } catch (error) {
    console.error("Logout error:", error);
    performLogoutCleanup();
    redirectToHome();
  }
};
