export const THINKING_OPTIONS = [
  { value: "disabled", label: "Disabled" },
  { value: "enabled", label: "Enabled" },
  { value: "adaptive", label: "Adaptive" },
  { value: "effort:low", label: "Effort: Low" },
  { value: "effort:medium", label: "Effort: Medium" },
  { value: "effort:high", label: "Effort: High" }
] as const;

export const VENDOR_OPTIONS = [
  { value: "anthropic", label: "Anthropic" },
  { value: "openai", label: "OpenAI" },
  { value: "openai_compat", label: "OpenAI-compatible (Ollama, vLLM, …)" }
] as const;

export const STATE_NAMES = [
  "clarifying",
  "specifying",
  "solving",
  "executing",
  "interpreting",
  "diagnosing"
] as const;
