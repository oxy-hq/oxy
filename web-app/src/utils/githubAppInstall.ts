/**
 * Opens a secure window to install a GitHub app
 * @param url The URL to open
 * @returns Window reference or null if blocked
 */
export const openSecureWindow = (url: string): Window | null => {
  const dualScreenLeft = window.screenLeft ?? window.screenX;
  const dualScreenTop = window.screenTop ?? window.screenY;
  const windowWidth = window.innerWidth ?? document.documentElement.clientWidth;
  const windowHeight = window.innerHeight ?? document.documentElement.clientHeight;

  // Calculate centered position
  const left = dualScreenLeft + (windowWidth - 600) / 2;
  const top = dualScreenTop + (windowHeight - 700) / 2;
  const features = ["popup=yes", "width=600", "height=700", `left=${left}`, `top=${top}`].join(",");

  const newWindow = window.open(url, "_blank", features);
  if (newWindow) {
    newWindow.focus(); // Bring popup to front
  }
  return newWindow;
};

/**
 * Opens the GitHub app installation page
 * @param installUrl The installation URL from the API
 * @returns Window reference or null if blocked
 */
export const openGitHubAppInstallation = async (installUrl: string): Promise<Window | null> => {
  return openSecureWindow(installUrl);
};

/**
 * Extract installation information from URL query parameters
 * Used when returning from GitHub App installation flow
 */
export const getInstallationInfoFromUrl = (): {
  installationId?: string;
  state?: string;
  code: string;
} => {
  const urlParams = new URLSearchParams(window.location.search);
  const installationId = urlParams.get("installation_id") || undefined;
  const code = urlParams.get("code") || "";
  const state = urlParams.get("state") || undefined;

  return { installationId, state, code };
};
