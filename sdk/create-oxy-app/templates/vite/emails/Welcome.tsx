// An email template rendered to HTML by @oxy-hq/sdk/email and sent by the
// `notify` function (../functions/notify.ts). It's a plain component: props in,
// HTML out. JSX compiles to preact (see emails/tsconfig.json) — preact runs
// inside the Oxy Functions isolate; React Email (react-dom) does not. Email HTML
// is tables + inline styles anyway, which is what this emits.

export function Welcome({ name }: { name: string }) {
  return (
    <html>
      <head />
      <body
        style={{ backgroundColor: "#f6f6f6", fontFamily: "system-ui, sans-serif", margin: "0", padding: "24px" }}
      >
        <div
          style={{ maxWidth: "480px", margin: "0 auto", backgroundColor: "#ffffff", padding: "24px", borderRadius: "8px" }}
        >
          <h1 style={{ fontSize: "20px", margin: "0 0 12px" }}>Welcome, {name}!</h1>
          <p style={{ color: "#334155", fontSize: "14px", lineHeight: "20px", margin: "0" }}>
            Thanks for joining {{APP_DISPLAY_NAME}}. We're glad you're here.
          </p>
        </div>
      </body>
    </html>
  );
}
