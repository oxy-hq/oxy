// "Update ready" in the shell's top bar.
//
// Shown only where a person can act on it and nothing else will: an app whose
// manifest says `performance.autoUpdate: "prompt"`, or an installed PWA
// (`display-mode: standalone`), which has no browser reload button. In `auto`
// mode in a browser tab the runtime reloads on its own at a safe moment, so a
// pill there would only be noise.

import { useEffect, useState } from "react";
import { useAppUpdate } from "../custom-app/update-hooks";

function useStandalone(): boolean {
  const [standalone, setStandalone] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(display-mode: standalone)");
    const update = () => setStandalone(query.matches);
    update();
    query.addEventListener?.("change", update);
    return () => query.removeEventListener?.("change", update);
  }, []);
  return standalone;
}

export function UpdatePill() {
  const update = useAppUpdate();
  const standalone = useStandalone();
  if (!update.available || (update.mode !== "prompt" && !standalone)) return null;
  return (
    <button
      type='button'
      data-testid='topbar-update-ready'
      className='oxy-shell-scope oxy-updatepill'
      onClick={() => update.apply()}
    >
      Update ready
    </button>
  );
}
