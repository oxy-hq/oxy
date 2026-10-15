// @vitest-environment jsdom

import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { AgentSkillsButton, AgentSkillsStrip, useAgentSkillsDismissed } from ".";
import { INSTALL_METHODS, QUICK_INSTALL } from "./installMethods";

afterEach(cleanup);
beforeEach(() => localStorage.clear());

describe("useAgentSkillsDismissed", () => {
  it("starts undismissed, and a dismissal survives a remount", () => {
    const first = renderHook(() => useAgentSkillsDismissed());
    expect(first.result.current[0]).toBe(false);
    act(() => first.result.current[1]());
    expect(first.result.current[0]).toBe(true);
    first.unmount();

    // A dismissal that lasts only until the next render is a banner that nags.
    const second = renderHook(() => useAgentSkillsDismissed());
    expect(second.result.current[0]).toBe(true);
  });
});

describe("AgentSkillsStrip", () => {
  it("leads with the command that works for any agent", () => {
    render(<AgentSkillsStrip onDismiss={() => {}} />);
    expect(screen.getByTestId("admin-agent-skills-strip").textContent).toContain(QUICK_INSTALL);
  });

  it("calls onDismiss from its close button", async () => {
    let dismissed = false;
    render(<AgentSkillsStrip onDismiss={() => (dismissed = true)} />);
    await userEvent.click(screen.getByTestId("admin-agent-skills-dismiss"));
    expect(dismissed).toBe(true);
  });
});

describe("AgentSkillsButton", () => {
  it("opens every install method, each with its commands", async () => {
    render(<AgentSkillsButton />);
    await userEvent.click(screen.getByTestId("admin-agent-skills-button"));
    expect(screen.getByTestId("admin-agent-skills-install")).toBeTruthy();

    for (const method of INSTALL_METHODS) {
      await userEvent.click(screen.getByRole("tab", { name: method.label }));
      const panel = screen.getByTestId(`admin-agent-skills-method-${method.id}`);
      for (const command of method.commands) expect(panel.textContent).toContain(command);
    }
  });
});
