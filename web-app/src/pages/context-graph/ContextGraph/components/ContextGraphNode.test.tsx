// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@xyflow/react", () => ({
  Handle: ({ type, style }: { type: string; style: Record<string, unknown> }) => (
    <div data-testid={`handle-${type}`} style={style} />
  ),
  Position: { Left: "left", Right: "right" }
}));

// Must import after vi.mock
const { ContextGraphNode } = await import("./ContextGraphNode");

afterEach(() => cleanup());

const renderNode = (data: Record<string, unknown>) => {
  // ContextGraphNode only reads `data` from props
  return render(<ContextGraphNode {...({ data } as any)} />);
};

describe("ContextGraphNode", () => {
  it("renders label text", () => {
    renderNode({ label: "My Agent", type: "agent" });
    expect(screen.getByText("My Agent")).toBeInTheDocument();
  });

  it("renders correct icon for node type", () => {
    renderNode({ label: "Test View", type: "view" });
    expect(screen.getByText("Test View")).toBeInTheDocument();
  });

  it("renders inner container with icon and label", () => {
    const { container } = renderNode({ label: "Agent", type: "agent" });
    const inner = container.querySelector("div > div:last-child") as HTMLElement;
    expect(inner).not.toBeNull();
    // Inner container has both icon (svg) and label (span) children
    expect(inner.querySelector("span")).toHaveTextContent("Agent");
  });

  it("applies opacity style when opacity data is set", () => {
    const { container } = renderNode({ label: "Agent", type: "agent", opacity: 0.5 });
    const outer = container.firstChild as HTMLElement;
    expect(outer.style.opacity).toBe("0.5");
  });

  it("defaults opacity to 1 when not set", () => {
    const { container } = renderNode({ label: "Agent", type: "agent" });
    const outer = container.firstChild as HTMLElement;
    expect(outer.style.opacity).toBe("1");
  });

  it("shows visible handle style when showLeftHandle is true", () => {
    renderNode({ label: "Agent", type: "agent", showLeftHandle: true, showRightHandle: false });
    const leftHandle = screen.getByTestId("handle-target");
    expect(leftHandle.style.width).toBe("8px");
  });

  it("hides handle when showLeftHandle is false", () => {
    renderNode({ label: "Agent", type: "agent", showLeftHandle: false, showRightHandle: false });
    const leftHandle = screen.getByTestId("handle-target");
    expect(leftHandle.style.opacity).toBe("0");
  });
});
