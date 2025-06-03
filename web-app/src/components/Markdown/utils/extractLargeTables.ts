export function extractLargeTables(markdown: string) {
  const tableRegex = /((?:^\|.*\|$\n?)+)/gm;

  const tables: string[][][] = [];
  let lastIndex = 0;
  const parts: string[] = [];
  let match: RegExpExecArray | null;

  while ((match = tableRegex.exec(markdown)) !== null) {
    const tableMarkdown = match[0];
    const start = match.index;
    const end = tableRegex.lastIndex;

    parts.push(markdown.slice(lastIndex, start));

    const tableData = parseMarkdownTable(tableMarkdown);
    if (tableData.length < 50) {
      parts.push(tableMarkdown);
      lastIndex = end;
      continue;
    }
    const tableId = tables.length;

    tables.push(tableData);

    parts.push(`\n:table_virtualized{table_id=${tableId}}\n`);

    lastIndex = end;
  }

  parts.push(markdown.slice(lastIndex));

  const newMarkdown = parts.join("");

  return { newMarkdown, tables };
}

function parseMarkdownTable(md: string): string[][] {
  const lines = md.trim().split("\n").filter(Boolean);

  const dataLines = lines.filter((line) => !line.match(/^\|[-\s|:]+$/));

  return dataLines.map((line) =>
    line
      .trim()
      .slice(1, -1)
      .split("|")
      .map((cell) => cell.trim()),
  );
}
