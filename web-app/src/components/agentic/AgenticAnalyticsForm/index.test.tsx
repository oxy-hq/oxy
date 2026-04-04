// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgenticAnalyticsForm, type AgenticFormData, type AgenticYamlData } from "./index";

afterEach(() => {
  cleanup();
});

const defaultData: AgenticFormData = {
  instructions: "",
  databases: [],
  llm: { ref: "claude", model: "claude-haiku-4-5", max_tokens: 8000, thinking: "disabled" },
  context: [],
  thinking: undefined,
  states: {}
};

// ─── Rendering ───────────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — rendering", () => {
  it("renders all top-level sections", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByText("Databases")).toBeInTheDocument();
    expect(screen.getByText("LLM Configuration")).toBeInTheDocument();
    expect(screen.getByText("Context")).toBeInTheDocument();
    expect(screen.getByText("State Overrides")).toBeInTheDocument();
    expect(screen.getByText("Validation")).toBeInTheDocument();
    expect(screen.getByText("Semantic Engine")).toBeInTheDocument();
  });

  it("renders the instructions textarea", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByPlaceholderText(/Global instructions injected/i)).toBeInTheDocument();
  });

  it("renders the global thinking mode select", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByText("Global Thinking Mode")).toBeInTheDocument();
  });

  it("pre-fills LLM fields from data", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByDisplayValue("claude")).toBeInTheDocument();
    expect(screen.getByDisplayValue("claude-haiku-4-5")).toBeInTheDocument();
    expect(screen.getByDisplayValue("8000")).toBeInTheDocument();
  });

  it("renders all six state override rows", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    for (const state of [
      "clarifying",
      "specifying",
      "solving",
      "executing",
      "interpreting",
      "diagnosing"
    ]) {
      expect(screen.getByText(state)).toBeInTheDocument();
    }
  });

  it("does not show extended thinking section by default when not in data", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.queryByText("Extended Thinking")).not.toBeInTheDocument();
    expect(screen.getByText("Add Extended Thinking")).toBeInTheDocument();
  });

  it("shows extended thinking section when data contains it", () => {
    const data: AgenticFormData = {
      ...defaultData,
      llm: {
        ...defaultData.llm,
        extended_thinking: { model: "claude-opus-4-6", thinking: "adaptive" }
      }
    };
    render(<AgenticAnalyticsForm data={data} />);
    expect(screen.getByText("Extended Thinking")).toBeInTheDocument();
    expect(screen.getByDisplayValue("claude-opus-4-6")).toBeInTheDocument();
  });
});

// ─── LLM Config ──────────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — LLM config", () => {
  it("renders vendor, api_key, and base_url fields", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByLabelText(/^Vendor/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/API Key/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Base URL/i)).toBeInTheDocument();
  });
});

// ─── State Overrides ─────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — state overrides", () => {
  it("expands a state row and shows instructions + model fields", async () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByText("specifying"));
    await waitFor(() =>
      expect(
        screen.getByPlaceholderText(/Additional instructions for this state only/i)
      ).toBeInTheDocument()
    );
    expect(screen.getByPlaceholderText(/inherits global if blank/i)).toBeInTheDocument();
  });
});

// ─── Databases ───────────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — databases", () => {
  it("shows empty state message when no databases", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByText(/No databases configured/)).toBeInTheDocument();
  });

  it("renders database inputs when data has entries", () => {
    const data: AgenticFormData = {
      ...defaultData,
      databases: [{ value: "training" }, { value: "production" }]
    };
    render(<AgenticAnalyticsForm data={data} />);
    expect(screen.getByDisplayValue("training")).toBeInTheDocument();
    expect(screen.getByDisplayValue("production")).toBeInTheDocument();
  });

  it("adds a new database input when Add button is clicked", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByRole("button", { name: /Add Database/i }));
    expect(screen.getAllByPlaceholderText(/Database name/i)).toHaveLength(1);
  });
});

// ─── Context globs ───────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — context globs", () => {
  it("shows empty state message when no context", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByText(/No context patterns defined/)).toBeInTheDocument();
  });

  it("renders glob inputs when data has entries", () => {
    const data: AgenticFormData = {
      ...defaultData,
      context: [{ value: "./semantics/**/*" }, { value: "./example_sql/*.sql" }]
    };
    render(<AgenticAnalyticsForm data={data} />);
    expect(screen.getByDisplayValue("./semantics/**/*")).toBeInTheDocument();
    expect(screen.getByDisplayValue("./example_sql/*.sql")).toBeInTheDocument();
  });

  it("adds a new glob input when Add Glob Pattern button is clicked", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByRole("button", { name: /Add Glob Pattern/i }));
    expect(screen.getAllByPlaceholderText(/semantics/i)).toHaveLength(1);
  });
});

// ─── Semantic Engine ─────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — semantic engine", () => {
  it("shows add button by default when no semantic engine data", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    expect(screen.getByRole("button", { name: /Add Semantic Engine/i })).toBeInTheDocument();
  });

  it("shows engine fields after clicking Add Semantic Engine", () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByRole("button", { name: /Add Semantic Engine/i }));
    expect(screen.getByText(/Vendor/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Base URL/)).toBeInTheDocument();
  });

  it("marks vendor and base_url as required with asterisk", () => {
    const data: AgenticFormData = { ...defaultData, semantic_engine: { vendor: "cube" } };
    render(<AgenticAnalyticsForm data={data} />);
    // Both required labels have * markers
    const vendorLabel = screen.getByText("*", { selector: "span.text-destructive" });
    expect(vendorLabel).toBeInTheDocument();
  });

  it("shows api_token field for cube vendor", () => {
    const data: AgenticFormData = {
      ...defaultData,
      semantic_engine: { vendor: "cube", base_url: "https://cube.example.com" }
    };
    render(<AgenticAnalyticsForm data={data} />);
    expect(screen.getByLabelText(/API Token/i)).toBeInTheDocument();
  });
});

// ─── Validation ──────────────────────────────────────────────────────────────

describe("AgenticAnalyticsForm — validation", () => {
  it("expands validation section on click", async () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByText("Validation"));
    await waitFor(() => expect(screen.getByText("After Specify")).toBeInTheDocument());
    expect(screen.getByText("After Solve")).toBeInTheDocument();
    expect(screen.getByText("After Execute")).toBeInTheDocument();
  });

  it("adds a rule to a stage and shows rule name select", async () => {
    render(<AgenticAnalyticsForm data={defaultData} />);
    fireEvent.click(screen.getByText("Validation"));
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: /Add Rule/i })).toHaveLength(3)
    );
    fireEvent.click(screen.getAllByRole("button", { name: /Add Rule/i })[0]);
    await waitFor(() => expect(screen.getByText("Rule Name")).toBeInTheDocument());
  });
});

// ─── onChange serialization ───────────────────────────────────────────────────

describe("AgenticAnalyticsForm — onChange serialization", () => {
  it("calls onChange with correct yaml shape after LLM ref change", async () => {
    const onChange = vi.fn<[AgenticYamlData], void>();
    render(<AgenticAnalyticsForm data={defaultData} onChange={onChange} />);
    const refInput = screen.getByDisplayValue("claude");
    fireEvent.change(refInput, { target: { value: "openai" } });
    fireEvent.blur(refInput);
    await waitFor(() => expect(onChange).toHaveBeenCalled(), { timeout: 1000 });
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(lastCall.llm?.ref).toBe("openai");
  });

  it("omits empty databases from onChange payload", async () => {
    const onChange = vi.fn<[AgenticYamlData], void>();
    render(<AgenticAnalyticsForm data={defaultData} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: /Add Database/i }));
    const refInput = screen.getByDisplayValue("claude");
    fireEvent.change(refInput, { target: { value: "claude" } });
    fireEvent.blur(refInput);
    await waitFor(() => expect(onChange).toHaveBeenCalled(), { timeout: 1000 });
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(lastCall.databases).toBeUndefined();
  });

  it("serializes context globs as a string array (not object array)", async () => {
    const onChange = vi.fn<[AgenticYamlData], void>();
    const data: AgenticFormData = {
      ...defaultData,
      context: [{ value: "./semantics/**/*" }]
    };
    render(<AgenticAnalyticsForm data={data} onChange={onChange} />);
    const refInput = screen.getByDisplayValue("claude");
    fireEvent.change(refInput, { target: { value: "claude2" } });
    fireEvent.blur(refInput);
    await waitFor(() => expect(onChange).toHaveBeenCalled(), { timeout: 1000 });
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(Array.isArray(lastCall.context)).toBe(true);
    expect(typeof lastCall.context?.[0]).toBe("string");
  });
});
