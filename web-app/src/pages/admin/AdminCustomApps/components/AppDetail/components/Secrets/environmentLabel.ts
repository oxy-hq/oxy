import { PRODUCTION } from "@/types/appEnvironments";

export const STAGING = "staging";

/** How an environment's name reads in the console: `Production`, `Staging`,
 *  or a sandbox by its own name (`dev-luong`), which is what `oxyc env` and
 *  the sandbox's host call it. */
export function environmentLabel(name: string): string {
  if (name === PRODUCTION) return "Production";
  if (name === STAGING) return "Staging";
  return `Sandbox ${name}`;
}
