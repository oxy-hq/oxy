// `@oxy-hq/sdk/email` — render an email template to an HTML string for use in
// an Oxy Function's `ctx.email.send`.
//
// You write email templates as plain JSX components (point JSX at preact with
// `jsxImportSource: "preact"` in the template's tsconfig — see the examples),
// then:
//
// ```ts
// import { render } from "@oxy-hq/sdk/email";
// import { Welcome } from "../emails/Welcome";
// await ctx.email.send({ to, subject, html: render(Welcome, { name }) });
// ```
//
// Rendering uses preact-render-to-string — pure JS, no react-dom/server, no node
// builtins, no Web Streams — so it bundles under esbuild `--platform=neutral`
// and runs inside the Oxy Functions isolate (React Email / react-dom cannot).
// Kept in a separate subpath entry so it never bloats the main SDK bundle.

import { type ComponentType, h } from "preact";
import { render as prerender } from "preact-render-to-string";

/** Render an email template component to an HTML string. */
export function render<P extends Record<string, unknown>>(
  Component: ComponentType<P>,
  props: P
): string {
  return prerender(h(Component, props));
}
