export const apiBaseURL = (() => {
  if (process.env.NODE_ENV === "development") {
    return import.meta.env.VITE_API_BASE_URL || "http://localhost:3000/api";
  }
  return `${window.location.origin}/api`;
})();
