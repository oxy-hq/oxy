import type { OpenBuilds } from "@/types/apps";
import { formatWindow } from "../availabilityTone";

const plural = (n: number, one: string, many: string) =>
  `${n.toLocaleString()} ${n === 1 ? one : many}`;

/**
 * One line: have open tabs picked up the last publish?
 *
 * "Not measured" is said as such and never drawn as "all on the live build" —
 * capture being off, a failed query, or nothing published to compare against
 * are open questions, not good news.
 */
export const OpenBuildsLine = ({ open }: { open: OpenBuilds | undefined }) => {
  if (!open) return null;
  const window = formatWindow(open.window_minutes);
  const text = (() => {
    switch (open.status) {
      case "not_measured":
        return "Open tabs: not measured — which build each session runs is unknown here.";
      case "none_seen":
        return `Open tabs: no session reported its build in the last ${window}.`;
      // "Seen" is load-bearing: a tab behind re-reports every 6 h only while it
      // can reach Oxy, so one offline or signed out longer than the window
      // drops out of this count rather than showing as older.
      case "all_live":
        return `Open tabs: all ${plural(open.live_sessions, "session", "sessions")} seen in the last ${window} are on the live build (a tab offline or signed out that long is not seen).`;
      case "some_older":
        return `Open tabs: ${plural(open.older_sessions, "session", "sessions")} on ${plural(
          open.older_builds,
          "older build",
          "older builds"
        )}, ${open.live_sessions.toLocaleString()} on the live build (last ${window}).`;
    }
  })();
  return (
    <p
      className={
        open.status === "some_older"
          ? "text-foreground text-xs leading-relaxed"
          : "text-muted-foreground text-xs leading-relaxed"
      }
      data-testid='admin-app-open-builds'
      data-status={open.status}
    >
      {text}
    </p>
  );
};
