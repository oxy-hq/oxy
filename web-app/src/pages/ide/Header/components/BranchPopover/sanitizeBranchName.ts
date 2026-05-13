// Normalise free-text input into a valid git branch name: collapse
// whitespace to hyphens and strip characters git rejects (~ ^ : ? * [ \).
export function sanitizeBranchName(raw: string): string {
  return raw
    .trim()
    .replace(/\s+/g, "-")
    .replace(/[~^:?*[\\ ]+/g, "")
    .replace(/\.{2,}/g, ".")
    .replace(/^[.-]+/, "")
    .replace(/\.+$/, "")
    .replace(/-+/g, "-");
}
