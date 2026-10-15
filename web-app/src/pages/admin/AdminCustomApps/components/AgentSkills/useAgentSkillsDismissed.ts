import { useCallback, useState } from "react";

const KEY = "oxy.admin.agentSkillsReminder.dismissed";

/**
 * Whether this browser has dismissed the agent-skills reminder.
 *
 * Per browser, not per account: it is a nudge, and showing it once more on a new machine
 * is the right failure. Storage access is guarded because private-mode Safari throws on
 * the accessor itself, and this runs in a `useState` initialiser.
 */
export function useAgentSkillsDismissed(): [dismissed: boolean, dismiss: () => void] {
  const [dismissed, setDismissed] = useState(() => {
    try {
      return localStorage.getItem(KEY) === "1";
    } catch {
      return false;
    }
  });
  const dismiss = useCallback(() => {
    setDismissed(true);
    try {
      localStorage.setItem(KEY, "1");
    } catch {
      /* the strip comes back next visit; nothing worse */
    }
  }, []);
  return [dismissed, dismiss];
}
