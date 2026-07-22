/** Join truthy class names. Local minimal `clsx` — the shell uses fixed
 *  namespaced classes, so no Tailwind-style merge logic is needed. */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
