import { describe, expect, it } from "vitest";
import { type FunctionError, readFunctionSseStream } from "./function-sse";

/** Build a fake `Response` whose body streams the given chunks verbatim. */
function sseResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  let i = 0;
  const body = {
    getReader() {
      return {
        read: async () =>
          i < chunks.length
            ? { done: false, value: encoder.encode(chunks[i++]) }
            : { done: true, value: undefined }
      };
    }
  };
  return { body } as unknown as Response;
}

describe("readFunctionSseStream", () => {
  it("resolves with the decoded result + empty logs from data + done frames", async () => {
    const resp = sseResponse([
      'event: data\ndata: {"stores":[{"store":"1"}]}\n\n',
      'event: done\ndata: {"status":200}\n\n'
    ]);
    await expect(readFunctionSseStream(resp)).resolves.toEqual({
      value: { stores: [{ store: "1" }] },
      logs: []
    });
  });

  it("collects log frames ahead of the terminal frame", async () => {
    const resp = sseResponse([
      'event: log\ndata: {"level":"info","message":"hello"}\n\n',
      'event: log\ndata: {"level":"error","message":"oops"}\n\n',
      'event: data\ndata: {"ok":true}\n\n',
      "event: done\ndata: {}\n\n"
    ]);
    await expect(readFunctionSseStream(resp)).resolves.toEqual({
      value: { ok: true },
      logs: [
        { level: "info", message: "hello" },
        { level: "error", message: "oops" }
      ]
    });
  });

  it("reassembles a frame split across chunk boundaries", async () => {
    const resp = sseResponse(["event: data\nda", 'ta: {"n":', "42}\n\nevent: done\ndata: {}\n\n"]);
    await expect(readFunctionSseStream(resp)).resolves.toEqual({ value: { n: 42 }, logs: [] });
  });

  it("rejects with server message + name, carrying logs up to the throw", async () => {
    const resp = sseResponse([
      'event: log\ndata: {"level":"info","message":"before throw"}\n\n',
      'event: error\ndata: {"error":"FunctionError","message":"boom"}\n\n'
    ]);
    await expect(readFunctionSseStream(resp)).rejects.toMatchObject({
      name: "FunctionError",
      message: "boom"
    });
    // The error carries the logs the function printed before it threw.
    const err = await readFunctionSseStream(
      sseResponse([
        'event: log\ndata: {"level":"info","message":"before throw"}\n\n',
        'event: error\ndata: {"error":"FunctionError","message":"boom"}\n\n'
      ])
    ).catch((e) => e as FunctionError);
    expect(err.logs).toEqual([{ level: "info", message: "before throw" }]);
  });

  it("rejects when the stream ends without a terminal event", async () => {
    const resp = sseResponse(['event: data\ndata: {"n":1}\n\n']);
    await expect(readFunctionSseStream(resp)).rejects.toThrow(
      "function stream ended without a terminal event"
    );
  });

  it("throws when the response has no body stream", async () => {
    await expect(readFunctionSseStream({ body: null } as unknown as Response)).rejects.toThrow(
      "function response has no body stream"
    );
  });
});
