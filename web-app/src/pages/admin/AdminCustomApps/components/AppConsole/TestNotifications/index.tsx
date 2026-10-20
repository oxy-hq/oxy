import { BellOff, TriangleAlert } from "lucide-react";
import { useAppNotificationKinds } from "@/hooks/api/customApps/useAppNotificationTest";
import { AdminAsync } from "@/pages/admin/components/AdminAsync";
import { AdminEmptyState } from "@/pages/admin/components/AdminEmptyState";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { CustomApp } from "@/types/apps";
import { ConsolePanel } from "../../ConsolePanel";
import { KindRow } from "./KindRow";

/**
 * "Test notifications": send yourself one notification as this app, to see it
 * reach a phone without writing a function that calls `ctx.notify`.
 *
 * It goes only to the person clicking, through the app's own send path: push and the
 * inbox, with their mutes applied. So muting a kind in the phone's Settings and sending
 * again shows nothing arrives, and signing the phone out does the same. A critical kind
 * ignores mutes, and its row says so.
 *
 * The rows are the kinds the app's live build declares — the same list a person mutes
 * in Settings. When the app cannot notify the caller at all (staff outside its org), the
 * server says so and the buttons are off, rather than letting a click fail.
 */
export const TestNotifications = ({ app }: { app: CustomApp }) => {
  const kinds = useAppNotificationKinds(app.id);

  return (
    <ConsolePanel
      id='notifications'
      title='Test notifications'
      question='Q1 — do its notifications arrive'
    >
      <div className='flex flex-col gap-2' data-testid='admin-app-notification-test'>
        <p className='text-muted-foreground text-xs'>
          Sends one notification as {app.name} to you only, by push and in your inbox. Your mutes
          apply. Sign in on a phone as yourself to see it arrive.
        </p>
        <AdminAsync
          query={kinds}
          noun='this app&rsquo;s notification kinds'
          rows={2}
          isEmpty={(data) => data.kinds.length === 0}
          empty={
            <AdminEmptyState
              icon={BellOff}
              title='This app declares no notification kinds.'
              description='Add them to oxy-app.json under notifications.kinds, or to a function’s notify.kinds, and publish.'
            />
          }
        >
          {(data) => (
            <>
              {!data.reachable && <NotReachable />}
              <ul className='flex flex-col gap-1.5' data-testid='admin-app-notification-test-kinds'>
                {data.kinds.map((kind) => (
                  <KindRow key={kind.kind} appId={app.id} kind={kind} disabled={!data.reachable} />
                ))}
              </ul>
            </>
          )}
        </AdminAsync>
      </div>
    </ConsolePanel>
  );
};

/** The app's audience does not include the caller, so a send would have nobody to tell. */
const NotReachable = () => (
  <div
    className='flex items-start gap-1.5 rounded-md border border-border/60 px-2.5 py-2 text-xs'
    data-testid='admin-app-notification-test-unreachable'
  >
    <TriangleAlert className={`mt-px size-3 shrink-0 ${ADMIN_TONE.warn.text}`} />
    <span className='text-muted-foreground'>
      This app can&rsquo;t notify you: you aren&rsquo;t a member of its org. Join the org to test on
      your own phone.
    </span>
  </div>
);
