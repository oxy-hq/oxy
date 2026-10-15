import type { MonitorNotify, MonitorNotifyPerson } from "@/types/metricAnomalies";

/**
 * Which insights the file's `notify:` block announces, as the words that
 * follow "New insights".
 *
 * `min_severity` is a floor, so `medium` has to read as "medium or above" —
 * "at medium severity" would say a high one is left out.
 */
export function announcedSeverity(notify: MonitorNotify): string {
  if (notify.min_severity === "high") return "at high severity";
  if (notify.min_severity === "medium") return "at medium severity or above";
  return "of any severity";
}

const ROLE_AUDIENCE = { owner: "the owners", admin: "the admins", member: "every member" } as const;

/** One `people` entry as a few words: an address, a role's holders, or a team. */
function personLabel(person: MonitorNotifyPerson): string | null {
  if (person.email) return person.email;
  if (person.role) return ROLE_AUDIENCE[person.role];
  if (person.team) return `team ${person.team}`;
  return null;
}

/** How many entries are named before the rest are counted ("and 2 more"). */
const NAMED = 2;

/**
 * Who the block tells in Oxygen, as the words after "sent to" — e.g.
 * "dana@acme.com and the admins", or "dana@acme.com, team Finance and 2 more".
 * Entries are described, not counted as people: a role or a team names many.
 * `null` when the block names nobody.
 */
export function notifiedPeople(notify: MonitorNotify): string | null {
  const labels = (notify.people ?? []).map(personLabel).filter((l): l is string => l !== null);
  if (labels.length === 0) return null;
  if (labels.length === 1) return labels[0];
  if (labels.length <= NAMED + 1) {
    return `${labels.slice(0, -1).join(", ")} and ${labels[labels.length - 1]}`;
  }
  return `${labels.slice(0, NAMED).join(", ")} and ${labels.length - NAMED} more`;
}
