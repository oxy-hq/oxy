// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock("./Preview", () => ({
  default: () => <div data-testid='analytics-preview' />
}));

vi.mock("./Tests", () => ({
  default: () => <div data-testid='analytics-tests'>No tests configured</div>
}));

// Radix Tabs doesn't fire onValueChange with plain fireEvent.click in jsdom.
// Replace with a minimal implementation that wires up onValueChange correctly.
let capturedOnValueChange: ((v: string) => void) | undefined;
vi.mock("@/components/ui/shadcn/tabs", () => ({
  Tabs: ({
    onValueChange,
    children
  }: {
    onValueChange: (v: string) => void;
    children: React.ReactNode;
  }) => {
    capturedOnValueChange = onValueChange;
    return <div>{children}</div>;
  },
  TabsList: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TabsTrigger: ({ value, children }: { value: string; children: React.ReactNode }) => (
    <button
      type='button'
      data-testid={`tab-${value}`}
      onClick={() => capturedOnValueChange?.(value)}
    >
      {children}
    </button>
  )
}));

afterEach(() => {
  cleanup();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("PreviewSection", () => {
  it("renders Preview and Test tab triggers", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);
    expect(screen.getByTestId("tab-preview")).toBeTruthy();
    expect(screen.getByTestId("tab-test")).toBeTruthy();
  });

  it("shows the analytics preview by default", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);
    expect(screen.getByTestId("analytics-preview")).toBeTruthy();
    expect(screen.queryByTestId("analytics-tests")).toBeNull();
  });

  it("switches to Tests component when Test tab is clicked", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);
    fireEvent.click(screen.getByTestId("tab-test"));
    expect(screen.getByTestId("analytics-tests")).toBeTruthy();
    expect(screen.queryByTestId("analytics-preview")).toBeNull();
  });

  it("shows the Clean button on the Preview tab", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);
    expect(screen.getByTitle("Clear conversation")).toBeTruthy();
  });

  it("does not show the Clean button on the Test tab", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);
    fireEvent.click(screen.getByTestId("tab-test"));
    expect(screen.queryByTitle("Clear conversation")).toBeNull();
  });

  it("Clean button remounts the preview (resets run state)", async () => {
    const { default: PreviewSection } = await import("./PreviewSection");
    const { rerender } = render(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);

    const previewBefore = screen.getByTestId("analytics-preview");
    fireEvent.click(screen.getByTitle("Clear conversation"));
    rerender(<PreviewSection pathb64='dGVzdA==' previewKey='k1' />);

    // After clicking Clean the component remounts — the DOM node is replaced
    const previewAfter = screen.getByTestId("analytics-preview");
    expect(previewAfter).not.toBe(previewBefore);
  });
});
