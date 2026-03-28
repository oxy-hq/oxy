// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HumanInputQuestion } from "@/services/api/analytics";
import SuspensionPrompt from "./SuspensionPrompt";

afterEach(() => {
  cleanup();
});

const q = (prompt: string, suggestions: string[] = []): HumanInputQuestion => ({
  prompt,
  suggestions
});

describe("SuspensionPrompt — input display logic", () => {
  it("renders the question prompt", () => {
    render(<SuspensionPrompt questions={[q("What date range?")]} onAnswer={vi.fn()} isAnswering={false} />);
    expect(screen.getByText("What date range?")).toBeTruthy();
  });

  it("renders a single textarea for one question", () => {
    render(<SuspensionPrompt questions={[q("Pick a metric")]} onAnswer={vi.fn()} isAnswering={false} />);
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
  });

  it("renders one textarea per question for multiple questions", () => {
    render(
      <SuspensionPrompt
        questions={[q("Question 1"), q("Question 2"), q("Question 3")]}
        onAnswer={vi.fn()}
        isAnswering={false}
      />
    );
    expect(screen.getAllByRole("textbox")).toHaveLength(3);
  });

  it("renders suggestion chips", () => {
    render(
      <SuspensionPrompt
        questions={[q("Pick one", ["Option A", "Option B"])]}
        onAnswer={vi.fn()}
        isAnswering={false}
      />
    );
    expect(screen.getByText("Option A")).toBeTruthy();
    expect(screen.getByText("Option B")).toBeTruthy();
  });

  it("calls onAnswer immediately when a suggestion chip is clicked for a single question", () => {
    const onAnswer = vi.fn();
    render(
      <SuspensionPrompt
        questions={[q("Pick one", ["Option A"])]}
        onAnswer={onAnswer}
        isAnswering={false}
      />
    );
    fireEvent.click(screen.getByText("Option A"));
    expect(onAnswer).toHaveBeenCalledWith("Option A");
  });

  it("fills textarea when suggestion chip is clicked for multiple questions", () => {
    const onAnswer = vi.fn();
    render(
      <SuspensionPrompt
        questions={[q("Q1", ["Chip1"]), q("Q2")]}
        onAnswer={onAnswer}
        isAnswering={false}
      />
    );
    fireEvent.click(screen.getByText("Chip1"));
    // chip click on multi-question fills the field, does not submit
    expect(onAnswer).not.toHaveBeenCalled();
    const [firstTextarea] = screen.getAllByRole("textbox");
    expect((firstTextarea as HTMLTextAreaElement).value).toBe("Chip1");
  });

  it("shows 'Send all answers' button for multiple questions", () => {
    render(
      <SuspensionPrompt
        questions={[q("Q1"), q("Q2")]}
        onAnswer={vi.fn()}
        isAnswering={false}
      />
    );
    expect(screen.getByText("Send all answers")).toBeTruthy();
  });

  it("submits combined answers for multiple questions", () => {
    const onAnswer = vi.fn();
    render(
      <SuspensionPrompt
        questions={[q("Q1"), q("Q2")]}
        onAnswer={onAnswer}
        isAnswering={false}
      />
    );
    const [ta1, ta2] = screen.getAllByRole("textbox") as HTMLTextAreaElement[];
    fireEvent.change(ta1, { target: { value: "Answer 1" } });
    fireEvent.change(ta2, { target: { value: "Answer 2" } });
    fireEvent.click(screen.getByText("Send all answers"));
    expect(onAnswer).toHaveBeenCalledWith("Q: Q1\nA: Answer 1\n\nQ: Q2\nA: Answer 2");
  });

  it("disables inputs and buttons while isAnswering", () => {
    render(
      <SuspensionPrompt
        questions={[q("Q1", ["chip"])]}
        onAnswer={vi.fn()}
        isAnswering={true}
      />
    );
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(true);
    expect((screen.getByText("chip") as HTMLButtonElement).disabled).toBe(true);
  });

  it("submits on Enter for a single question", () => {
    const onAnswer = vi.fn();
    render(<SuspensionPrompt questions={[q("Q1")]} onAnswer={onAnswer} isAnswering={false} />);
    const textarea = screen.getByRole("textbox");
    fireEvent.change(textarea, { target: { value: "my answer" } });
    fireEvent.keyDown(textarea, { key: "Enter", shiftKey: false });
    expect(onAnswer).toHaveBeenCalledWith("my answer");
  });
});
