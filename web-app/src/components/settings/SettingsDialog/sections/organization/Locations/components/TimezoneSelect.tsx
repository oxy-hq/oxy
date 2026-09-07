import { Combobox } from "@/components/ui/shadcn/combobox";
import { TIMEZONES } from "../utils";

// The IANA list is 400+ entries; build the items once, not per keystroke of
// the form around this picker.
const ITEMS = TIMEZONES.map((tz) => ({ value: tz, label: tz }));

export function TimezoneSelect({
  value,
  onValueChange,
  testId
}: {
  value: string;
  onValueChange: (value: string) => void;
  testId: string;
}) {
  return (
    <div data-testid={testId}>
      <Combobox
        items={ITEMS}
        value={value}
        onValueChange={onValueChange}
        placeholder='Pick a timezone'
        searchPlaceholder='Search timezones...'
      />
    </div>
  );
}
