import { Switch } from "@/components/ui/shadcn/switch";
import { useSetNotificationPreference } from "@/hooks/api/notifications";
import type { AppPreferences, NotificationReach } from "@/types/notifications";
import { CHANNELS, humanizeKind } from "../utils";

/**
 * One app's kinds, each with a Push / Email / Slack switch (on = not muted).
 *
 * A critical kind is delivered whatever is muted, so its switches show on and
 * cannot move. Email cannot move either for an account with no address.
 */
export function AppKinds({ prefs, reach }: { prefs: AppPreferences; reach: NotificationReach }) {
  const setPreference = useSetNotificationPreference();
  const { app, kinds } = prefs;

  return (
    <div className='rounded-lg border' data-testid={`notifications-app-${app.slug}`}>
      <div className='flex items-center gap-2 border-b bg-muted/30 px-4 py-2'>
        <span className='min-w-0 flex-1 truncate font-medium text-sm'>{app.name}</span>
        {CHANNELS.map((c) => (
          <span key={c.value} className='w-14 text-center text-muted-foreground text-xs'>
            {c.label}
          </span>
        ))}
      </div>
      {kinds.map((k) => {
        const label = humanizeKind(k.kind);
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
                  data-testid={`notifications-critical-${app.slug}-${k.kind}`}
                >
                  Critical — always delivered
                </span>
              )}
            </span>
            {CHANNELS.map((c) => (
              <span key={c.value} className='flex w-14 justify-center'>
                <Switch
                  checked={k.critical || !k.muted.includes(c.value)}
                  disabled={k.critical || (c.value === "email" && !reach.email)}
                  aria-label={`${label}: ${c.label}`}
                  data-testid={`notifications-pref-${app.slug}-${k.kind}-${c.value}`}
                  onCheckedChange={(checked) =>
                    setPreference.mutate({
                      app_id: app.id,
                      kind: k.kind,
                      channel: c.value,
                      muted: !checked
                    })
                  }
                />
              </span>
            ))}
          </div>
        );
      })}
    </div>
  );
}
