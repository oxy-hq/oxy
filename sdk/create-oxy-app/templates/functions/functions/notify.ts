// notify — an example Oxy Function: a server-side handler that emails the
// INVOKING USER a welcome message. Oxy bundles `functions/*` at `oxy publish`
// and runs them in a sandboxed isolate with a data-plane `ctx`.
//
// It's declared in oxy-app.json under `functions.notify` with
// `"email": { "send": true }` — the fail-closed capability that lets it call
// `ctx.email.send`. Without that capability the host rejects the send.
//
// SECURITY — the recipient is derived server-side (`ctx.user.email`), NEVER
// taken from the request body. Mail goes from the platform's shared verified
// sender, so a caller-supplied `to` is an open relay / spam cannon (and burns
// SES reputation for everyone). Always derive recipients server-side — the
// invoking user, or an address looked up from your own data. Template DATA
// (like the display name below) can come from the request; the recipient
// must not.
//
// Invoke it from the frontend (no recipient — it goes to you):
//   const { invoke } = useFunction("notify");
//   await invoke({ name: "Ada" });
// or add a `schedule` in oxy-app.json to run it in the background.
//
// LOCAL DEV: on a cloud-mode dev server, set OXY_APP_EMAIL_LOCAL_TEST=1 to
// preview the rendered email in the browser instead of sending it via SES.
// To iterate on the template alone, `pnpm email:dev` renders it with sample
// props — no server needed.

import type { OxyFunctionContext, OxyFunctionRequest } from "@oxy-hq/sdk";
import { render } from "@oxy-hq/sdk/email";
import { Welcome } from "../emails/Welcome";

interface NotifyBody {
  name?: string;
}

export default async function notify(
  req: OxyFunctionRequest,
  ctx: OxyFunctionContext
): Promise<Response> {
  const { name } = JSON.parse(req.body || "{}") as NotifyBody;

  const { messageId } = await ctx.email.send({
    to: ctx.user.email,
    subject: "Welcome!",
    html: render(Welcome, { name: name || ctx.user.email })
  });

  return Response.json({ ok: true, messageId, sentTo: ctx.user.email });
}
