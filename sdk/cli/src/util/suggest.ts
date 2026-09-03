/**
 * "Did you mean…?" for a token that is not a command.
 *
 * ITS OWN MODULE, and that is the whole point. It used to live in `main.ts`,
 * which meant the test had to import the ENTRY module to reach it — and that
 * in turn needed an `isEntryPoint()` guard so importing did not start the CLI
 * on the importer's argv. That guard compared `resolve(process.argv[1])`
 * against `import.meta.url`, which is wrong the moment an npm bin is involved:
 * `npm i -g` installs `…/bin/oxyc` as a SYMLINK to `dist/main.mjs`, `resolve()`
 * does not follow links, so the comparison fails, `main()` never runs, and the
 * binary prints nothing and exits 0 — the worst possible shape for a tool an
 * agent branches on the exit code of, and invisible to a test that spawns the
 * real path.
 *
 * Moving the function here deletes the guard and the bug with it. Nothing in
 * `main.ts` needs to be importable any more.
 */

/**
 * The nearest command name within one or two edits, if there is one.
 *
 * Two is the usual cutoff for a suggestion: it catches a transposition
 * (`rotues`) and a doubled or dropped letter, without claiming that
 * `pokehouse` was a mistyped `doctor`. Short tokens get a tighter bound, since
 * two edits on a four-letter word is most of the word.
 *
 * TIES GO TO THE FIRST NAME IN THE LIST, deliberately and not by accident:
 * `<` rather than `<=` below. The caller passes commands in declaration order,
 * so a tie is resolved the same way on every run and by every machine — an
 * arbitrary but STABLE answer. Resolving it by "whichever came last" would
 * make the suggestion depend on how `buildProgram` happens to be ordered, and
 * that ordering changes whenever somebody adds a command.
 */
export function didYouMeanCommand(typed: string, known: string[]): string | undefined {
  const budget = typed.length <= 4 ? 1 : 2;
  let best: string | undefined;
  let bestDistance = budget + 1;
  for (const name of known) {
    const d = editDistance(typed, name);
    if (d < bestDistance) {
      bestDistance = d;
      best = name;
    }
  }
  return bestDistance <= budget ? best : undefined;
}

/** Levenshtein, iterative and allocation-light — the list is ~25 names. */
export function editDistance(a: string, b: string): number {
  let previous = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const current = [i];
    for (let j = 1; j <= b.length; j++) {
      current[j] = Math.min(
        (previous[j] ?? 0) + 1,
        (current[j - 1] ?? 0) + 1,
        (previous[j - 1] ?? 0) + (a[i - 1] === b[j - 1] ? 0 : 1)
      );
    }
    previous = current;
  }
  return previous[b.length] ?? Number.MAX_SAFE_INTEGER;
}
