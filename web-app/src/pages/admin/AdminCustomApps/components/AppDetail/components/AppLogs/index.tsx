import { ClientErrors } from "./components/ClientErrors";
import { FunctionLogs } from "./components/FunctionLogs";

/**
 * Logs — what the app actually did, from both sides of the wire.
 *
 * Server side is persisted `ctx.log()` / `console.*` from Oxy Functions; before
 * these were stored, a route call's output lived only inside the HTTP response
 * that carried it and vanished the moment the caller navigated away. Client
 * side is uncaught browser errors with **message and stack** — the platform
 * used to keep names and counts only, which made a white-screened app report
 * `{TypeError: 3}` and nothing anyone could act on.
 *
 * Stacks are source-mapped on the server. The maps are no longer served to
 * browsers, so this panel is the only place they are applied.
 */
export const AppLogs = ({ orgSlug, appSlug }: { orgSlug: string; appSlug: string }) => (
  <div className='space-y-4 p-4 pt-0' data-testid='admin-app-logs'>
    <section className='space-y-2'>
      <h3 className='font-semibold text-sm'>Client errors</h3>
      <ClientErrors orgSlug={orgSlug} appSlug={appSlug} />
    </section>
    <section className='space-y-2'>
      <h3 className='font-semibold text-sm'>Function output</h3>
      <FunctionLogs orgSlug={orgSlug} appSlug={appSlug} />
    </section>
  </div>
);
