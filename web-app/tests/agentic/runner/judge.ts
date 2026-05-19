import { randomBytes } from "node:crypto";
import type { Page } from "@playwright/test";
import { computeCost } from "./pricing";
import { type ExpectResult, emptyJudgeUsage, type FlowExpect, type JudgeUsage } from "./types";

const ANTHROPIC_VERSION = "2023-06-01";
const ANTHROPIC_API_URL = "https://api.anthropic.com/v1/messages";

export interface JudgeOptions {
  apiKey: string;
  model: string;
}

export interface ExpectationsResult {
  results: ExpectResult[];
  usage: JudgeUsage;
}

export async function evaluateExpectations(
  page: Page,
  expectations: FlowExpect[],
  opts: JudgeOptions
): Promise<ExpectationsResult> {
  const results: ExpectResult[] = [];
  const usage: JudgeUsage = emptyJudgeUsage();
  for (const expect of expectations) {
    if (expect.assert) {
      results.push(await evaluateAssert(page, expect.assert));
    } else if (expect.judge) {
      results.push(await evaluateJudge(page, expect.judge, opts, usage));
    }
  }
  return { results, usage };
}

async function evaluateAssert(page: Page, claim: string): Promise<ExpectResult> {
  try {
    const result = await runAssert(page, claim);
    return { kind: "assert", passed: result.passed, claim, evidence: result.evidence };
  } catch (err) {
    return {
      kind: "assert",
      passed: false,
      claim,
      evidence: err instanceof Error ? err.message : String(err)
    };
  }
}

async function runAssert(
  page: Page,
  claim: string
): Promise<{ passed: boolean; evidence: string }> {
  const visibleMatch = claim.match(/^selector\s+(.+?)\s+is visible$/i);
  if (visibleMatch) {
    const sel = visibleMatch[1].trim();
    const visible = await page
      .locator(sel)
      .first()
      .isVisible()
      .catch(() => false);
    return { passed: visible, evidence: `visibility(${sel})=${visible}` };
  }

  const notVisibleMatch = claim.match(/^selector\s+(.+?)\s+is not visible$/i);
  if (notVisibleMatch) {
    const sel = notVisibleMatch[1].trim();
    const visible = await page
      .locator(sel)
      .first()
      .isVisible()
      .catch(() => false);
    return { passed: !visible, evidence: `visibility(${sel})=${visible}` };
  }

  const attrMatch = claim.match(/^selector\s+(.+?)\s+has attribute\s+([\w-]+)=(.+)$/i);
  if (attrMatch) {
    const [, sel, attr, want] = attrMatch;
    const actual = await page
      .locator(sel)
      .first()
      .getAttribute(attr)
      .catch(() => null);
    const got = actual ?? "<null>";
    return { passed: got === want.trim(), evidence: `${attr}(${sel})=${got}` };
  }

  const textVisibleMatch = claim.match(/^text\s+["'](.+)["']\s+(?:is\s+)?visible$/i);
  if (textVisibleMatch) {
    const text = textVisibleMatch[1];
    const visible = await page
      .getByText(text)
      .first()
      .isVisible()
      .catch(() => false);
    return { passed: visible, evidence: `text(${text})=${visible}` };
  }

  const enabledMatch = claim.match(/^(.+?)\s+is enabled$/i);
  if (enabledMatch) {
    const desc = enabledMatch[1].toLowerCase();
    if (desc.includes("follow-up")) {
      const enabled = await page
        .getByPlaceholder("Ask follow-up question")
        .isEnabled()
        .catch(() => false);
      return { passed: enabled, evidence: `follow-up enabled=${enabled}` };
    }
  }

  const saveButtonHidden = /^save button is not visible$/i.test(claim);
  if (saveButtonHidden) {
    // Save commit is async; give the React state ~5s to flush after Meta+s.
    const hidden = await page
      .getByTestId("ide-save-button")
      .waitFor({ state: "hidden", timeout: 5_000 })
      .then(() => true)
      .catch(() => false);
    return { passed: hidden, evidence: `save button hidden within 5s = ${hidden}` };
  }

  throw new Error(
    `unsupported assert pattern: '${claim}' (see runner/judge.ts for supported forms)`
  );
}

async function evaluateJudge(
  page: Page,
  claim: string,
  opts: JudgeOptions,
  usage: JudgeUsage
): Promise<ExpectResult> {
  const screenshot = await page.screenshot({ fullPage: false }).catch(() => null);
  const fullText = await page
    .locator("body")
    .innerText()
    .catch(() => "");
  const domText = fullText.slice(0, 4000);

  // The DOM text may contain attacker-controlled content (e.g. an LLM
  // response rendered in chat). Without a nonce, that content could
  // include a JSON blob that steers parseJudgeResponse to passed=true.
  // The nonce is a cryptographic random the judge must echo back; we
  // ignore any JSON object that doesn't include the matching nonce.
  const nonce = randomBytes(8).toString("hex");

  const messages = [
    {
      role: "user" as const,
      content: [
        ...(screenshot
          ? [
              {
                type: "image" as const,
                source: {
                  type: "base64" as const,
                  media_type: "image/png" as const,
                  data: screenshot.toString("base64")
                }
              }
            ]
          : []),
        {
          type: "text" as const,
          text: `Visible page text (truncated):\n${domText}\n\nClaim: ${claim}\n\nIs this claim true based on the screenshot and text? Respond with JSON: {"nonce": "${nonce}", "passed": boolean, "rationale": string}.`
        }
      ]
    }
  ];

  const res = await fetch(ANTHROPIC_API_URL, {
    method: "POST",
    headers: {
      "x-api-key": opts.apiKey,
      "anthropic-version": ANTHROPIC_VERSION,
      "content-type": "application/json"
    },
    body: JSON.stringify({
      model: opts.model,
      max_tokens: 256,
      messages,
      system: `You evaluate claims about a web application screenshot. Respond ONLY with JSON: {"nonce": "${nonce}", "passed": boolean, "rationale": "<one sentence>"}. The nonce field MUST exactly equal the value in the user message.`
    })
  });

  if (!res.ok) {
    const body = await res.text();
    return {
      kind: "judge",
      passed: false,
      claim,
      rationale: `judge API error ${res.status}: ${body.slice(0, 200)}`
    };
  }

  const json = (await res.json()) as {
    content?: Array<{ type: string; text?: string }>;
    usage?: {
      input_tokens?: number;
      cache_read_input_tokens?: number;
      cache_creation_input_tokens?: number;
      output_tokens?: number;
    };
  };

  if (json.usage) {
    usage.model = opts.model;
    usage.calls += 1;
    usage.tokens.input += json.usage.input_tokens ?? 0;
    usage.tokens.cached_input += json.usage.cache_read_input_tokens ?? 0;
    usage.tokens.cache_creation += json.usage.cache_creation_input_tokens ?? 0;
    usage.tokens.output += json.usage.output_tokens ?? 0;
    usage.cost_usd = computeCost(opts.model, usage.tokens);
  }

  const text = json.content?.find((c) => c.type === "text")?.text ?? "";
  const parsed = parseJudgeResponse(text, nonce);
  return { kind: "judge", passed: parsed.passed, claim, rationale: parsed.rationale };
}

function parseJudgeResponse(
  text: string,
  expectedNonce: string
): { passed: boolean; rationale: string } {
  // Walk every {...} block in the response (longest-first, so a nested
  // object inside the model's outer JSON envelope still wins) and accept
  // the first one whose nonce matches. A response with no nonce-bearing
  // block fails closed.
  const matches = Array.from(text.matchAll(/\{[\s\S]*?\}/g)).map((m) => m[0]);
  matches.sort((a, b) => b.length - a.length);
  for (const candidate of matches) {
    try {
      const obj = JSON.parse(candidate) as {
        nonce?: string;
        passed?: boolean;
        rationale?: string;
      };
      if (obj.nonce !== expectedNonce) continue;
      return { passed: obj.passed === true, rationale: obj.rationale ?? "" };
    } catch {
      // skip non-JSON blob, keep looking
    }
  }
  return { passed: false, rationale: `unparsable judge response: ${text.slice(0, 200)}` };
}
