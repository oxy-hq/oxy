import { useMemo, useState } from "react";
import { cn } from "@/libs/shadcn/utils";
import type { CustomerApp } from "@/types/apps";
import { resolveBundleUrl } from "../resolveBundleUrl";

/**
 * The app's favicon, served from its own bundle. We hit the canonical
 * same-origin path (`…/customer-apps/<org>/<app>/favicon.ico`) so the session
 * cookie rides along and even a draft app resolves; anything that fails to
 * decode (no favicon, HTML fallback, 401) drops to a neutral monogram. Kept on
 * design tokens — no derived colors — so the grid stays calm.
 */
export const AppFavicon = ({
  app,
  size = "sm",
  className
}: {
  app: CustomerApp;
  size?: "sm" | "lg";
  className?: string;
}) => {
  const [failed, setFailed] = useState(false);
  const src = useMemo(() => {
    try {
      const base = resolveBundleUrl(app.url);
      return new URL("favicon.ico", base.endsWith("/") ? base : `${base}/`).toString();
    } catch {
      return null;
    }
  }, [app.url]);

  const box = size === "lg" ? "size-8 text-xs" : "size-5 text-[10px]";

  if (!src || failed) {
    return (
      <span
        aria-hidden
        className={cn(
          "flex shrink-0 items-center justify-center rounded bg-muted font-medium text-muted-foreground",
          box,
          className
        )}
      >
        {app.name.slice(0, 1).toUpperCase()}
      </span>
    );
  }
  return (
    <img
      src={src}
      alt=''
      onError={() => setFailed(true)}
      className={cn("shrink-0 rounded object-contain", box, className)}
    />
  );
};
