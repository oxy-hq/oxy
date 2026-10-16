import { useMemo } from "react";
import { useSetNotificationPreference } from "@/hooks/api/notifications";
import type { NotificationReach, SourceNode } from "@/types/notifications";
import { CHANNELS, prefRows } from "../utils";
import { PrefRowView } from "./PrefRowView";

/**
 * One source — Oxygen or an app — as its tree: the source's own switches,
 * then each category (indented) and its kinds. A switch mutes its whole
 * subtree; a kind's switch mutes that kind alone.
 */
export function SourceGroup({ source, reach }: { source: SourceNode; reach: NotificationReach }) {
  const setPreference = useSetNotificationPreference();
  const rows = useMemo(() => prefRows(source), [source]);
  const appId = source.app?.id ?? null;
  const testKey = source.app?.slug ?? "oxygen";

  return (
    <div className='rounded-lg border' data-testid={`notifications-group-${testKey}`}>
      <div className='flex items-center gap-2 border-b px-4 py-1.5'>
        <span className='min-w-0 flex-1' />
        {CHANNELS.map((c) => (
          <span key={c.value} className='w-12 text-center text-muted-foreground text-xs sm:w-14'>
            {c.label}
          </span>
        ))}
      </div>
      {rows.map((row) => (
        <PrefRowView
          key={row.key}
          row={row}
          source={source.label}
          testKey={testKey}
          reach={reach}
          onChange={(channel, muted) =>
            setPreference.mutate({ app_id: appId, ...row.target, channel, muted })
          }
        />
      ))}
    </div>
  );
}
