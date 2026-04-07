// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GraphFilterPanel } from "./GraphFilterPanel";

afterEach(() => cleanup());

const defaultProps = {
  focusType: "auto" as const,
  onFocusTypeChange: vi.fn(),
  expandAll: false,
  onExpandAllChange: vi.fn(),
  focusedNodeId: null as string | null,
  onReset: vi.fn()
};

describe("GraphFilterPanel", () => {
  it("renders focus type selector", () => {
    render(<GraphFilterPanel {...defaultProps} />);
    expect(screen.getByText("Focus View")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-filter-type")).toBeInTheDocument();
  });

  it("renders expand all checkbox", () => {
    render(<GraphFilterPanel {...defaultProps} />);
    expect(screen.getByText("Expand all connected")).toBeInTheDocument();
  });

  it("expand checkbox is disabled when focusedNodeId is null", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId={null} />);
    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).toBeDisabled();
  });

  it("expand checkbox is enabled when focusedNodeId is set", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId='node-1' />);
    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).not.toBeDisabled();
  });

  it("calls onExpandAllChange when checkbox toggled", () => {
    const onExpandAllChange = vi.fn();
    render(
      <GraphFilterPanel
        {...defaultProps}
        focusedNodeId='node-1'
        onExpandAllChange={onExpandAllChange}
      />
    );
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onExpandAllChange).toHaveBeenCalledWith(true);
  });

  it("reset button is hidden when focusedNodeId is null", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId={null} />);
    expect(screen.queryByText("Reset View")).not.toBeInTheDocument();
  });

  it("reset button is visible when focusedNodeId is set", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId='node-1' />);
    expect(screen.getByText("Reset View")).toBeInTheDocument();
  });

  it("calls onReset when reset button clicked", () => {
    const onReset = vi.fn();
    render(<GraphFilterPanel {...defaultProps} focusedNodeId='node-1' onReset={onReset} />);
    fireEvent.click(screen.getByText("Reset View"));
    expect(onReset).toHaveBeenCalled();
  });
});
