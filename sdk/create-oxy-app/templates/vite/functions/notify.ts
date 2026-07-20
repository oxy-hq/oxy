// notify — an example Oxy Function (server-side handler) that emails the
// INVOKING USER a welcome message. Oxy bundles `functions/*` at `oxy publish`
// and runs them in a sandboxed isolate with a data-plane `ctx`.
//
// It's declared in oxy-app.json under `functions.notify` with
// `"email": { "send": true }` — the fail-closed capability that lets it call
// `ctx.email.send`. Without that capability the host rejects the send.
//
// SECURITY — the recipient is derived server-side (`ctx.user.email`), NEVER
// taken from the request body. On a route function, do NOT forward an untrusted
// request-body recipient to `ctx.email.send`: mail goes from the platform's
// shared verified sender, so a caller-supplied `to` is an open relay / spam
// cannon (and burns SES reputation for everyone). Always derive recipients
// server-side — the invoking user, or an address looked up from your own data —
// or filter to your own users.
//
// Invoke it from the frontend (no recipient — it goes to you):
//   const { invoke } = useFunction("notify");
//   await invoke({ name: "Ada" });
// or add a `schedule` in oxy-app.json to run it in the background.
//
// LOCAL DEV: under `oxy serve --local`, Oxy opens the rendered email in your
// browser instead of sending — no flag needed. On a cloud-mode dev server, set
// OXY_APP_EMAIL_LOCAL_TEST=1 to preview instead of sending via SES.

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

  // Recipient is the invoking user — never a caller-supplied `to` (see the note
  // above). Template DATA (the display name) can come from the request; the
  // recipient must not.
  const { messageId } = await ctx.email.send({
    to: ctx.user.email,
    subject: "Welcome!",
    html: render(Welcome, { name: name || ctx.user.email })
  });

  return Response.json({ ok: true, messageId });
}
