// @vitest-environment jsdom

import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Table, TableBody } from "@/components/ui/shadcn/table";
import type { AirhouseFleetRow as Row } from "@/services/api/airhouseAdmin";
import { AirhouseFleetRow } from "./AirhouseFleetRow";

const row = (over: Partial<Row> = {}): Row => ({
  workspace_id: "3c6e0b8a-9c15-224a-8236-000000000001",
  workspace_name: "Pokehouse",
  org_id: "11111111-2222-3333-4444-555555555555",
  org_name: "Pokehouse HQ",
  status: "active",
  tenant_id: "oxy-ws-3c6e0b8a",
  bucket: "oxy-airhouse-prod",
  prefix: "ws/3c6e0b8a",
  service_account_ready: true,
  sa_rotated_at: "2026-08-01T00:00:00Z",
  created_at: "2026-01-04T00:00:00Z",
  service_account_id: "sa-9f2c11ab",
  sa_created_at: "2026-01-04T00:00:00Z",
  bearer_max_role: "admin",
  bearer_max_ttl_secs: 86_400,
  ...over
});

afterEach(cleanup);

const renderRow = (r: Row, expanded = false) => {
  const onToggle = vi.fn();
  render(
    <Table>
      <TableBody>
        <AirhouseFleetRow row={r} expanded={expanded} onToggle={onToggle} />
      </TableBody>
    </Table>
  );
  return { onToggle };
};

describe("AirhouseFleetRow", () => {
  it("carries its severity on the row, so the rail and the filter cannot disagree", () => {
    renderRow(row({ service_account_ready: false }));
    expect(screen.getByTestId(/admin-airhouse-row-/)).toHaveAttribute("data-severity", "broken");
  });

  it("keeps the detail closed until asked", () => {
    const r = row();
    renderRow(r);
    expect(screen.queryByTestId(`admin-airhouse-detail-${r.workspace_id}`)).toBeNull();
  });

  it("opens on click", async () => {
    const r = row();
    const { onToggle } = renderRow(r);
    await userEvent.click(screen.getByTestId(`admin-airhouse-row-${r.workspace_id}`));
    expect(onToggle).toHaveBeenCalledOnce();
  });

  /**
   * An operator can tab to every `CopyableId` inside the row; without this they
   * could not reach the control that opens the strip those ids sit above. The
   * click test above passes either way, so it is asserted separately.
   */
  it("opens from the keyboard, not just the mouse", async () => {
    const r = row();
    const { onToggle } = renderRow(r);
    const tr = screen.getByTestId(`admin-airhouse-row-${r.workspace_id}`);

    expect(tr).toHaveAttribute("tabindex", "0");
    expect(tr).toHaveAttribute("aria-expanded", "false");

    tr.focus();
    expect(tr).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onToggle).toHaveBeenCalledOnce();

    await userEvent.keyboard(" ");
    expect(onToggle).toHaveBeenCalledTimes(2);
  });

  /**
   * The row's keydown handler must not swallow a nested button's activation.
   *
   * `CopyableId` stops its own *click* from reaching the row, which is what
   * makes the mouse safe — and says nothing about the keyboard. An unguarded
   * handler on the `<tr>` also fires for Enter pressed inside the copy button,
   * and its `preventDefault` cancels the activation click a `<button>` turns
   * that key into: nothing is copied, silently, and the row toggles instead.
   */
  it("leaves a nested copy button's own keys alone", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true
    });

    const r = row();
    const { onToggle } = renderRow(r);
    const copy = screen.getByRole("button", { name: `Copy ${r.tenant_id}` });

    // The mouse path first, so a failure below means the KEYBOARD is broken
    // rather than copying being broken outright — otherwise this test would go
    // red for a reason it does not name.
    await userEvent.click(copy);
    expect(writeText).toHaveBeenCalledWith(r.tenant_id);
    expect(onToggle).not.toHaveBeenCalled();
    writeText.mockClear();

    copy.focus();
    expect(copy).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(writeText).toHaveBeenCalledWith(r.tenant_id);
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("tells assistive tech when it is open", () => {
    const r = row();
    renderRow(r, true);
    expect(screen.getByTestId(`admin-airhouse-row-${r.workspace_id}`)).toHaveAttribute(
      "aria-expanded",
      "true"
    );
  });

  /**
   * The point of the strip: the facts an operator would otherwise open psql
   * for. Asserted by stable id rather than display copy, so rewording a label
   * does not fail the test but dropping a field does.
   */
  it("shows every fact the psql session would have", () => {
    const r = row();
    renderRow(r, true);
    const detail = screen.getByTestId(`admin-airhouse-detail-${r.workspace_id}`);
    for (const id of [
      "workspace-id",
      "org-id",
      "service-account",
      "role",
      "ttl",
      "bucket",
      "prefix",
      "created",
      "sa-created",
      "sa-rotated"
    ]) {
      expect(within(detail).getByTestId(`admin-airhouse-fact-${id}`)).toBeInTheDocument();
    }
    expect(within(detail).getByTestId("admin-airhouse-fact-role")).toHaveTextContent("admin");
    expect(within(detail).getByTestId("admin-airhouse-fact-ttl")).toHaveTextContent("24h");
    // Absolute dates in the strip: the collapsed row gives a magnitude for
    // scanning, the strip gives something to search a log for.
    expect(within(detail).getByTestId("admin-airhouse-fact-created")).toHaveTextContent(
      "2026-01-04"
    );
  });

  it("says an unbound service account is unbound, not blank", () => {
    const r = row({ service_account_id: null, service_account_ready: false });
    renderRow(r, true);
    const detail = screen.getByTestId(`admin-airhouse-detail-${r.workspace_id}`);
    expect(within(detail).getByTestId("admin-airhouse-fact-service-account")).toHaveTextContent(
      "not bound"
    );
  });
});
