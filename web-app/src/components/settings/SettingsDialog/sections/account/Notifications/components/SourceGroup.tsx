import { useMemo } from "react";
import { useUpdateNotificationSettings } from "@/hooks/api/notifications";
import type { NodePatch, SettingsReach, SettingsSource } from "@/types/notifications";
import { CHANNELS, type PrefRow, prefRows } from "../utils";
import { PrefRowView } from "./PrefRowView";

/**
 * One source — Oxygen or an app — as its tree: the source's own switches,
 * then each category (indented) and its kinds. What a row sets, everything
 * under it inherits until a row lower down sets its own.
 */
export function SourceGroup({ source, reach }: { source: SettingsSource; reach: SettingsReach }) {
  const update = useUpdateNotificationSettings();
  const rows = useMemo(() => prefRows(source), [source]);
  const appId = source.app?.id ?? null;
  const testKey = source.app?.slug ?? "oxygen";
  const write = (row: PrefRow, change: Pick<NodePatch, "channels" | "min_level">) =>
    update.mutate({ nodes: [{ app_id: appId, ...row.target, ...change }] });

  return (
    <div className='rounded-lg border' data-testid={`notifications-group-${testKey}`}>
      <div className='flex items-center gap-2 border-b py-1.5 pr-2 pl-4'>
        <span className='min-w-0 flex-1' />
        {CHANNELS.map((c) => (
          <span key={c.value} className='w-12 text-center text-muted-foreground text-xs sm:w-14'>
            {c.label}
          </span>
        ))}
        <span className='w-7' />
      </div>
      {rows.map((row) => (
        <PrefRowView
          key={row.key}
          row={row}
          source={source.label}
          testKey={testKey}
          reach={reach}
          onChannel={(channel, on) => write(row, { channels: { [channel]: on } })}
          onFloor={(level) => write(row, { min_level: level })}
        />
      ))}
    </div>
  );
}
