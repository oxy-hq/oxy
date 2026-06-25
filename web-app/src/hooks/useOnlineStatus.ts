import { useEffect, useState } from "react";

/**
 * Tracks `navigator.onLine`, updating on the browser's `online`/`offline`
 * events. Drives the top-bar system indicator ("Sys: Connected"). This is a
 * client-side connectivity signal only — a real backend health ping can
 * replace it later without changing the indicator's consumers.
 */
export default function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(() =>
    typeof navigator === "undefined" ? true : navigator.onLine
  );
  useEffect(() => {
    const goOnline = () => setOnline(true);
    const goOffline = () => setOnline(false);
    window.addEventListener("online", goOnline);
    window.addEventListener("offline", goOffline);
    return () => {
      window.removeEventListener("online", goOnline);
      window.removeEventListener("offline", goOffline);
    };
  }, []);
  return online;
}
