// Child processes for git and gh. Output is buffered; a failure throws with stderr.

import { execFile } from "node:child_process";

const MAX_BUFFER = 256 * 1024 * 1024;

export interface RunOptions {
  cwd?: string;
  input?: string;
  allowFail?: boolean;
}

export interface RunResult {
  code: number;
  stdout: string;
  stderr: string;
}

export function run(
  cmd: string,
  args: readonly string[],
  { cwd, input, allowFail = false }: RunOptions = {}
): Promise<RunResult> {
  return new Promise((resolve, reject) => {
    const child = execFile(
      cmd,
      args,
      { cwd, maxBuffer: MAX_BUFFER, encoding: "utf8" },
      (err, stdout, stderr) => {
        if (err && !allowFail) {
          err.message = `${cmd} ${args.slice(0, 4).join(" ")} failed: ${stderr || err.message}`;
          reject(err);
          return;
        }
        const code = err ? (typeof err.code === "number" ? err.code : 1) : 0;
        resolve({ code, stdout, stderr });
      }
    );
    if (input !== undefined) {
      child.stdin?.write(input);
      child.stdin?.end();
    }
  });
}

export type Git = (args: readonly string[], opts?: RunOptions) => Promise<string>;

export const git: Git = (args, opts) =>
  run("git", ["-c", "core.fsmonitor=false", ...args], opts).then((r) => r.stdout);
