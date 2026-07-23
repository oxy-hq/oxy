import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OxyAnswer } from "./react";

// Proves the built OxyAnswer renders GFM tables as real <table> markup
// (the bug: pipe rows fell through to the paragraph collector and showed
// as raw text). Rendering the actual component end-to-end, not just the
// parse helpers.
describe("OxyAnswer table rendering", () => {
  const md = [
    "Here is a breakdown by location:",
    "",
    "| Location | Net Sales | Orders |",
    "|---|---|---|",
    "| Palo Alto | 6,371 | 219 |",
    "| Almaden | 4,402 | 159 |"
  ].join("\n");

  const html = renderToStaticMarkup(<OxyAnswer answer={md} state='done' threadUrl={null} />);

  it("emits a real table element, not raw pipe text", () => {
    expect(html).toContain("<table");
    expect(html).toContain("<thead");
    expect(html).toContain("<tbody");
    // header + data cells present
    expect(html).toContain("Location");
    expect(html).toContain("Palo Alto");
    expect(html).toContain("6,371");
    expect(html).toContain("Almaden");
  });

  it("does not leave the delimiter row or literal pipes in the output", () => {
    // The prose line still renders, but the pipe table must not appear as
    // one run of literal '| Location | Net Sales |' text.
    expect(html).not.toContain("| Location | Net Sales | Orders |");
    expect(html).not.toContain("|---|---|---|");
  });

  it("still renders the surrounding prose", () => {
    expect(html).toContain("Here is a breakdown by location");
  });
});
