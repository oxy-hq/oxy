/**
 * Which of the two audiences is reading, and the styling that follows from it.
 *
 * One tool, two readers. A human at a terminal wants colour, alignment and a
 * spinner; an agent — or a pipe — wants markdown and plain bytes. The test is
 * whether the stream is a TTY, per clig.dev, and it is asked *per stream*:
 * stdout is routinely piped while stderr stays attached to the terminal, and
 * a run that dropped colour from its progress messages because its data was
 * being piped would be answering the wrong question.
 */

import pc from "picocolors";

/** True when stdout is a terminal — i.e. a human is reading the *data*. */
export function stdoutIsTty(): boolean {
  return Boolean(process.stdout.isTTY);
}

/** True when stderr is a terminal — i.e. a human is reading the *progress*. */
export function stderrIsTty(): boolean {
  return Boolean(process.stderr.isTTY);
}

/**
 * Whether to emit ANSI colour.
 *
 * `NO_COLOR` (any non-empty value) wins over everything — it is the whole
 * point of the convention that a user can turn colour off without the tool
 * getting a vote. `FORCE_COLOR` is the other direction, for CI logs that
 * render ANSI. Absent both, the stream decides.
 */
export function colorEnabled(stream: "stdout" | "stderr" = "stdout"): boolean {
  if (process.env.NO_COLOR) return false;
  if (process.env.FORCE_COLOR && process.env.FORCE_COLOR !== "0") return true;
  return stream === "stdout" ? stdoutIsTty() : stderrIsTty();
}

/**
 * Colour helpers that no-op when colour is off.
 *
 * picocolors already checks `NO_COLOR` and TTY-ness itself, but only against
 * stdout — so a message written to stderr while stdout is piped would come
 * out uncoloured for a human who is looking straight at it. Routing every
 * style through here keeps the per-stream answer above authoritative.
 */
function styler(stream: "stdout" | "stderr") {
  const on = colorEnabled(stream);
  const wrap =
    (fn: (s: string) => string) =>
    (s: string): string =>
      on ? fn(s) : s;
  return {
    bold: wrap(pc.bold),
    dim: wrap(pc.dim),
    red: wrap(pc.red),
    green: wrap(pc.green),
    yellow: wrap(pc.yellow),
    blue: wrap(pc.blue),
    cyan: wrap(pc.cyan),
    magenta: wrap(pc.magenta),
    gray: wrap(pc.gray),
    underline: wrap(pc.underline)
  };
}

/** Styles for data written to stdout. */
export const out = styler("stdout");
/** Styles for messages written to stderr. */
export const err = styler("stderr");

/**
 * How wide to lay a table or wrap a paragraph.
 *
 * Falls back to 100 rather than 80 when there is no terminal: the non-TTY
 * reader is a pipe or an agent, neither of which has an 80-column limit, and
 * a route path plus its description does not fit in 80.
 */
export function terminalWidth(): number {
  const cols = process.stdout.columns;
  if (typeof cols === "number" && cols > 20) return cols;
  return 100;
}
