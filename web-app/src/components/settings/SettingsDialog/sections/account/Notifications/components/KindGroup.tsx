import { Switch } from "@/components/ui/shadcn/switch";
import { useSetNotificationPreference } from "@/hooks/api/notifications";
import type { KindPreference, NotificationReach } from "@/types/notifications";
import { CHANNELS, kindLabel } from "../utils";

interface KindGroupProps {
  /** The heading: an app's name, or "Oxygen" for the platform's own kinds. */
  title: string;
  /** The app the kinds belong to; `null` for Oxygen's own. */
  appId: string | null;
  /** Names the group's test ids: an app's slug, or `oxygen`. */
  testKey: string;
  kinds: KindPreference[];
  reach: NotificationReach;
}

/**
 * One sender's kinds — an app's, or Oxygen's own — each with a Push / Email /
 * Slack switch (on = not muted).
 *
 * A critical kind is delivered whatever is muted, so its switches show on and
 * cannot move. Email cannot move either for an account with no address. A
 * channel the kind is never sent on gets no switch at all.
 */
export function KindGroup({ title, appId, testKey, kinds, reach }: KindGroupProps) {
  const setPreference = useSetNotificationPreference();

  return (
    <div className='rounded-lg border' data-testid={`notifications-group-${testKey}`}>
      <div className='flex items-center gap-2 border-b bg-muted/30 px-4 py-2'>
        <span className='min-w-0 flex-1 truncate font-medium text-sm'>{title}</span>
        {CHANNELS.map((c) => (
          <span key={c.value} className='w-12 text-center text-muted-foreground text-xs sm:w-14'>
            {c.label}
          </span>
        ))}
      </div>
      {kinds.map((k) => {
        const label = kindLabel(k.kind);
        return (
          <div
            key={k.kind}
            className='flex items-center gap-2 border-b px-4 py-2.5 last:border-b-0'
          >
            <span className='flex min-w-0 flex-1 flex-col'>
              <span className='text-sm'>{label}</span>
              {k.critical && (
                <span
                  className='text-muted-foreground text-xs'
                  data-testid={`notifications-critical-${testKey}-${k.kind}`}
                >
                  Critical — always delivered
                </span>
              )}
            </span>
            {CHANNELS.map((c) => (
              <span key={c.value} className='flex w-12 justify-center sm:w-14'>
                {k.channels && !k.channels.includes(c.value) ? (
                  <span
                    className='text-muted-foreground text-xs'
                    data-testid={`notifications-pref-${testKey}-${k.kind}-${c.value}-none`}
                  >
                    <span aria-hidden>—</span>
                    <span className='sr-only'>{`${title}, ${label}: not sent by ${c.label}`}</span>
                  </span>
                ) : (
                  <Switch
                    checked={k.critical || !k.muted.includes(c.value)}
                    disabled={k.critical || (c.value === "email" && !reach.email)}
                    aria-label={`${title}, ${label}: ${c.label}`}
                    data-testid={`notifications-pref-${testKey}-${k.kind}-${c.value}`}
                    onCheckedChange={(checked) =>
                      setPreference.mutate({
                        app_id: appId,
                        kind: k.kind,
                        channel: c.value,
                        muted: !checked
                      })
                    }
                  />
                )}
              </span>
            ))}
          </div>
        );
      })}
    </div>
  );
}
