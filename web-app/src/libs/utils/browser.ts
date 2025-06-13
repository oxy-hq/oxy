import Cookies from "js-cookie";

/**
 * Clears all cookies from the current domain and common parent domains.
 * Note: This function cannot clear HttpOnly cookies as they are inaccessible to client-side JavaScript.
 * HttpOnly cookies require server-side handling for removal.
 */
export const clearAllCookies = (): void => {
  const cookies = document.cookie.split(";");
  for (const cookie of cookies) {
    const eqPos = cookie.indexOf("=");
    const name = eqPos > -1 ? cookie.slice(0, eqPos).trim() : cookie.trim();

    if (name) {
      Cookies.remove(name);
      Cookies.remove(name, { path: "/" });
      Cookies.remove(name, { path: "/", domain: window.location.hostname });
      const domain = window.location.hostname.split(".").slice(-2).join(".");
      Cookies.remove(name, { path: "/", domain: `.${domain}` });
    }
  }
};

/**
 * Redirects to the application's home page
 */
export const redirectToHome = (): void => {
  window.location.href = window.location.origin;
};

/**
 * Clears all browser storage (localStorage, sessionStorage)
 */
export const clearBrowserStorage = (): void => {
  localStorage.clear();
  sessionStorage.clear();
};
