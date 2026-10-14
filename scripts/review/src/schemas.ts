// JSON schemas for each pass's structured answer. Field order is the order the model
// writes in: reasoning comes before the conclusion it supports.

import { CATEGORIES, SEVERITIES } from "../config.ts";

const str = (description: string) => ({ type: "string", description });

export const FINDINGS_SCHEMA = {
  type: "object",
  additionalProperties: false,
  required: ["findings", "not_reviewed"],
  properties: {
    findings: {
      type: "array",
      items: {
        type: "object",
        additionalProperties: false,
        required: [
          "reasoning",
          "title",
          "file",
          "line",
          "category",
          "severity",
          "confidence",
          "fix",
          "rule"
        ],
        properties: {
          reasoning: str(
            "Evidence first: the changed line, what it does, and the concrete failure."
          ),
          title: str("One line naming the defect."),
          file: str("Path from the repository root."),
          line: { type: "integer", description: "Line in the head version of the file." },
          end_line: { type: "integer" },
          category: { type: "string", enum: CATEGORIES },
          severity: { type: "string", enum: SEVERITIES },
          confidence: { type: "number", minimum: 0, maximum: 1 },
          fix: str("The concrete change."),
          rule: str("Repo guidance this breaks (CLAUDE.md section, skill, R-* id), or empty.")
        }
      }
    },
    not_reviewed: { type: "array", items: { type: "string" } }
  }
};

export const VERIFY_SCHEMA = {
  type: "object",
  additionalProperties: false,
  required: ["results"],
  properties: {
    results: {
      type: "array",
      items: {
        type: "object",
        additionalProperties: false,
        required: ["id", "reason", "verdict", "severity"],
        properties: {
          id: { type: "string" },
          reason: str("What in the code settles it."),
          verdict: { type: "string", enum: ["confirmed", "rejected", "uncertain"] },
          severity: { type: "string", enum: SEVERITIES },
          duplicate_of: { type: "string" }
        }
      }
    }
  }
};

export const RECHECK_SCHEMA = {
  type: "object",
  additionalProperties: false,
  required: ["results"],
  properties: {
    results: {
      type: "array",
      items: {
        type: "object",
        additionalProperties: false,
        required: ["id", "reason", "status"],
        properties: {
          id: { type: "string" },
          reason: str("One or two sentences from the code."),
          status: { type: "string", enum: ["fixed", "obsolete", "withdrawn", "open"] }
        }
      }
    }
  }
};
