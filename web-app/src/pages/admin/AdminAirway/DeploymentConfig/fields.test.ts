import { describe, expect, it } from "vitest";
import type { AirwayDeploymentValues } from "@/services/api/airwayConfig";
import {
  DEPLOYMENT_FIELDS,
  type DeploymentDraft,
  draftFromValues,
  isDirty,
  UNSET,
  valuesFromDraft
} from "./fields";

const EMPTY: AirwayDeploymentValues = {
  timeout_secs: null,
  max_retries: null,
  user_agent: null,
  retry_initial_delay_ms: null,
  retry_max_delay_secs: null,
  retry_backoff_factor: null,
  tls_ca_cert: null,
  tls_client_cert: null,
  tls_client_key_file: null,
  tls_danger_accept_invalid_certs: null
};

function draft(over: Partial<DeploymentDraft> = {}): DeploymentDraft {
  return { ...draftFromValues(EMPTY), ...over } as DeploymentDraft;
}

describe("valuesFromDraft", () => {
  /**
   * The regression this whole feature keeps having. An emptied input is
   * "airway's default", which is `null` on the wire — not `0`, not `""`, not
   * `false`. Asserted for every field at once so a new one cannot be added
   * with the wrong empty case.
   */
  it("maps every emptied field to null, never to a zero value", () => {
    const { values, invalid } = valuesFromDraft(draft());
    expect(invalid).toEqual([]);
    expect(values).toEqual(EMPTY);
    for (const field of DEPLOYMENT_FIELDS) {
      expect(values[field.key]).toBeNull();
    }
  });

  it("keeps an explicit zero, which is a different thing from an empty field", () => {
    const { values, invalid } = valuesFromDraft(draft({ max_retries: "0" }));
    expect(invalid).toEqual([]);
    expect(values.max_retries).toBe(0);
  });

  it("parses each kind in the unit its field name states", () => {
    const { values, invalid } = valuesFromDraft(
      draft({
        timeout_secs: "90",
        retry_initial_delay_ms: "250",
        retry_backoff_factor: "1.5",
        user_agent: "oxy-airway/1.0",
        tls_danger_accept_invalid_certs: "true"
      })
    );
    expect(invalid).toEqual([]);
    expect(values.timeout_secs).toBe(90);
    expect(values.retry_initial_delay_ms).toBe(250);
    expect(values.retry_backoff_factor).toBe(1.5);
    expect(values.user_agent).toBe("oxy-airway/1.0");
    expect(values.tls_danger_accept_invalid_certs).toBe(true);
  });

  /**
   * Text that is not a number of the field's kind is reported, not coerced.
   * `Number("")` is 0 and `parseInt("12abc")` is 12 — both would turn a typo
   * into a saved setting nobody chose.
   */
  it("reports unparseable text instead of coercing it", () => {
    const { values, invalid } = valuesFromDraft(
      draft({ timeout_secs: "abc", retry_backoff_factor: "1.5", max_retries: "1.5" })
    );
    // Reported in DEPLOYMENT_FIELDS order, so the list is stable to render.
    expect(invalid).toEqual(["timeout_secs", "max_retries"]);
    expect(values.timeout_secs).toBeNull();
    expect(values.retry_backoff_factor).toBe(1.5);
  });

  it("treats whitespace as empty rather than as a value", () => {
    const { values, invalid } = valuesFromDraft(draft({ user_agent: "   " }));
    expect(invalid).toEqual([]);
    expect(values.user_agent).toBeNull();
  });
});

describe("draftFromValues", () => {
  it("renders null as the unset sentinel, not as '0' or 'null'", () => {
    const d = draftFromValues({ ...EMPTY, timeout_secs: 30 });
    expect(d.timeout_secs).toBe("30");
    expect(d.max_retries).toBe(UNSET);
    expect(d.tls_danger_accept_invalid_certs).toBe(UNSET);
  });

  it("round-trips a fully configured row", () => {
    const values: AirwayDeploymentValues = {
      timeout_secs: 90,
      max_retries: 7,
      user_agent: "oxy-airway/1.0",
      retry_initial_delay_ms: 250,
      retry_max_delay_secs: 60,
      retry_backoff_factor: 1.5,
      tls_ca_cert: "/etc/pki/ca.pem",
      tls_client_cert: "/etc/pki/client.pem",
      tls_client_key_file: "/etc/pki/client.key",
      tls_danger_accept_invalid_certs: true
    };
    expect(valuesFromDraft(draftFromValues(values)).values).toEqual(values);
  });
});

describe("isDirty", () => {
  it("is false for an untouched draft and true once a field changes", () => {
    const stored: AirwayDeploymentValues = { ...EMPTY, timeout_secs: 30 };
    expect(isDirty(draftFromValues(stored), stored)).toBe(false);
    expect(isDirty({ ...draftFromValues(stored), timeout_secs: "31" }, stored)).toBe(true);
  });

  /** Clearing a set field is an edit — the un-set direction has to arm Save too. */
  it("is true when a stored value is cleared", () => {
    const stored: AirwayDeploymentValues = { ...EMPTY, timeout_secs: 30 };
    expect(isDirty({ ...draftFromValues(stored), timeout_secs: UNSET }, stored)).toBe(true);
  });

  /**
   * The draft is seeded once and keeps the operator's literal spelling, while
   * the row round-trips normalised. Comparing text left Save armed forever
   * after a successful save of `1.50` — an enabled button for a change that has
   * already been made, whose only effect is to write the same row again.
   */
  it("is false for text that differs from the stored value only in spelling", () => {
    const stored: AirwayDeploymentValues = { ...EMPTY, retry_backoff_factor: 1.5 };
    expect(isDirty({ ...draftFromValues(stored), retry_backoff_factor: "1.50" }, stored)).toBe(
      false
    );
    expect(isDirty({ ...draftFromValues(stored), retry_backoff_factor: " 1.5 " }, stored)).toBe(
      false
    );
    // Still a real edit when the *value* changes, not just its spelling.
    expect(isDirty({ ...draftFromValues(stored), retry_backoff_factor: "1.6" }, stored)).toBe(true);
  });

  /** `0` is a chosen value and `UNSET` is airway's default — never equal. */
  it("distinguishes an explicit zero from an unset field", () => {
    const stored: AirwayDeploymentValues = { ...EMPTY, max_retries: 0 };
    expect(isDirty({ ...draftFromValues(stored), max_retries: UNSET }, stored)).toBe(true);
    expect(isDirty(draftFromValues(stored), stored)).toBe(false);
  });
});

describe("DEPLOYMENT_FIELDS", () => {
  /**
   * Only settings airway reads. These four have zero occurrences in airway's
   * source, so a control for one would be accepted, saved and inert.
   */
  it("offers no knob airway does not read", () => {
    const keys = DEPLOYMENT_FIELDS.map((f) => f.key as string);
    for (const inert of [
      "max_rewind",
      "cursor_lag_floor",
      "allow_unversioned_writes",
      "partition_repull_budget",
      "tls_server_name",
      "tls_enabled"
    ]) {
      expect(keys).not.toContain(inert);
    }
    expect(keys).toHaveLength(10);
  });

  it("states a unit for every duration", () => {
    for (const key of ["timeout_secs", "retry_initial_delay_ms", "retry_max_delay_secs"]) {
      const field = DEPLOYMENT_FIELDS.find((f) => f.key === key);
      expect(field?.unit, `${key} must state its unit`).toBeTruthy();
    }
  });
});
