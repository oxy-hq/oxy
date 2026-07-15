import { useEffect } from "react";
import { toast } from "sonner";
import { isNewVersionDeployed } from "@/libs/utils/appVersion";

const POLL_INTERVAL_MS = 5 * 60 * 1000;

/**
 * Polls /version.json (emitted at build time next to the bundle) and prompts
 * for a reload when the server is running a newer build than this tab, so
 * long-lived tabs learn about deploys before they trip over a missing chunk.
 * Checks on a 5-minute interval and whenever the tab becomes visible again.
 */
export default function useVersionCheck() {
  useEffect(() => {
    // The dev server has no version.json and no deploys to detect.
    if (import.meta.env.DEV) return;

    let notified = false;
    const check = async () => {
      if (notified || !(await isNewVersionDeployed())) return;
      notified = true;
      toast.info("A new version of Oxygen is available", {
        description: "Reload to get the latest version.",
        duration: Number.POSITIVE_INFINITY,
        action: {
          label: "Reload",
          onClick: () => window.location.reload()
        }
      });
    };

    const intervalId = window.setInterval(() => void check(), POLL_INTERVAL_MS);
    const onVisibilityChange = () => {
      if (!document.hidden) void check();
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);
}
