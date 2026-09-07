// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useForm } from "react-hook-form";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { DRIVER_COLUMN_NAMES, defaultNewWorldValues, type SimulationFormValues } from "../schema";
import { ColumnNameField } from "./ColumnNameField";

// Radix's select scrolls the active item into view on open. jsdom has no
// layout, so without this the listbox throws from a timer after the test.
beforeAll(() => {
  Element.prototype.scrollIntoView = () => {};
});

afterEach(cleanup);

function Harness({ driver }: { driver: string }) {
  const { control, formState, watch } = useForm<SimulationFormValues>({
    defaultValues: {
      ...defaultNewWorldValues(),
      mechanism: { ...defaultNewWorldValues().mechanism, driver }
    }
  });
  return (
    <>
      <ColumnNameField
        name='mechanism.driver'
        label='Driver'
        control={control}
        errors={formState.errors}
        options={DRIVER_COLUMN_NAMES}
      />
      {/* The registered value, so a test can assert what the form would post
          rather than what the trigger happens to be showing. */}
      <output data-testid='value'>{watch("mechanism.driver")}</output>
    </>
  );
}

describe("ColumnNameField", () => {
  it("shows a preset value on the trigger, with no free-text input", () => {
    render(<Harness driver='marketing_spend' />);

    expect(screen.getByRole("combobox")).toHaveTextContent("marketing_spend");
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("opens in custom mode for a name outside the list, keeping it intact", () => {
    render(<Harness driver='legacy_media_spend' />);

    expect(screen.getByRole("textbox")).toHaveValue("legacy_media_spend");
    expect(screen.getByTestId("value")).toHaveTextContent("legacy_media_spend");
  });

  it("offers every preset plus the custom escape hatch", () => {
    render(<Harness driver='ad_spend' />);

    fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });

    // "Custom…" has to stay on the list: the backend accepts any bare column
    // name, so a select that only offered presets would narrow what a world
    // can be built from.
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      ...DRIVER_COLUMN_NAMES,
      "Custom…"
    ]);
  });
});
