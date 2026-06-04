import { useEffect } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { QB_CONNECT_MESSAGE } from "@/services/api/quickbooks";

/**
 * Landing page the backend OAuth callback redirects to after it has stored
 * the refresh token. Two modes:
 * - `popup`: post the captured realm id back to the opener (which validates
 *   `event.origin`) and close the window.
 * - `redirect` (mobile): navigate back to `return_path`, carrying the result
 *   as query params so the originating page can finish the flow.
 */
export default function QuickBooksConnected() {
  const [params] = useSearchParams();
  const navigate = useNavigate();

  useEffect(() => {
    const mode = params.get("mode") ?? "popup";
    const realmId = params.get("realm_id") ?? "";
    const returnPath = params.get("return_path");

    if (mode === "popup" && window.opener) {
      window.opener.postMessage({ type: QB_CONNECT_MESSAGE, realmId }, window.location.origin);
      window.close();
      return;
    }

    const dest = returnPath || "/";
    const sep = dest.includes("?") ? "&" : "?";
    navigate(`${dest}${sep}qb_connected=ok&realm_id=${encodeURIComponent(realmId)}`, {
      replace: true
    });
  }, [params, navigate]);

  return (
    <div className='flex h-screen items-center justify-center text-muted-foreground text-sm'>
      QuickBooks connected — you can close this window.
    </div>
  );
}
