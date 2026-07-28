// Minimal `text/event-stream` reader for the world-model streams.
//
// Unlike `function-sse.ts` (which reads a single terminal function result),
// the world-model `instance-detail` / `measure-breakdown` endpoints emit a
// sequence of `kind`-tagged JSON events on the default (unnamed) SSE event,
// terminating with a `{ kind: "done" }` frame and then closing the stream.
// This reader parses each `data:` frame's JSON and hands it to `onEvent`; the
// caller folds the events into accumulated state. It resolves when the stream
// closes (or the signal aborts) — the hook decides what "done" means.

/**
 * Read a `text/event-stream` response, invoking `onEvent` with each parsed
 * JSON frame. Frames that fail to parse are skipped (a malformed frame must
 * not tear down the whole stream). Resolves when the body closes.
 */
export async function readJsonSseStream<E>(
  resp: Response,
  onEvent: (event: E) => void
): Promise<void> {
  const reader = resp.body?.getReader();
  if (!reader) {
    throw new Error("SSE response has no body stream");
  }
  const decoder = new TextDecoder();
  let buffer = "";

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let sep: number;
    while ((sep = buffer.indexOf("\n\n")) !== -1) {
      const frame = buffer.slice(0, sep);
      buffer = buffer.slice(sep + 2);
      let data = "";
      for (const line of frame.split("\n")) {
        // Ignore `event:`/`id:`/`retry:` lines — the world-model streams put
        // everything in `data:` on the default event.
        if (line.startsWith("data:")) data += line.slice(5).trim();
      }
      if (!data) continue;
      let parsed: E;
      try {
        parsed = JSON.parse(data) as E;
      } catch {
        continue;
      }
      onEvent(parsed);
    }
  }
}
