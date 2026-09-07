import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import type { FrontlineStaff } from "@/types/frontline";

const GRID_CLASS = "grid grid-cols-2 gap-2";

/**
 * The shift board: every name on this kiosk's roster as a tile big enough to
 * hit with a thumb. Single-select — a worker picks themself, then enters a PIN.
 */
interface CrewRosterPickerProps {
  staff: FrontlineStaff[];
  /** Identifier of the picked crew member; empty when nobody is picked yet. */
  selected: string;
  onSelect: (identifier: string) => void;
  disabled?: boolean;
}

const CrewRosterPicker = ({ staff, selected, onSelect, disabled }: CrewRosterPickerProps) => (
  <ToggleGroup
    type='single'
    value={selected}
    // Radix clears the value when the pressed tile is tapped again; on a
    // kiosk a second tap on your own name must not un-pick you.
    onValueChange={(value) => {
      if (value) {
        onSelect(value);
      }
    }}
    disabled={disabled}
    aria-label='Your name'
    // Negative margin + padding so focus rings survive the scroll clip.
    className={`-m-1 max-h-72 overflow-y-auto p-1 ${GRID_CLASS}`}
  >
    {staff.map((member) => (
      <ToggleGroupItem
        key={member.identifier}
        value={member.identifier}
        variant='outline'
        className='h-auto min-h-12 px-2 py-2 text-center leading-tight data-[state=on]:border-primary data-[state=on]:bg-primary data-[state=on]:text-primary-foreground data-[state=on]:hover:bg-primary/90 data-[state=on]:hover:text-primary-foreground'
        data-testid={`login-crew-staff-${member.identifier}`}
      >
        {member.name}
      </ToggleGroupItem>
    ))}
  </ToggleGroup>
);

/** Same grid, no names yet — keeps the board from jumping when the roster lands. */
export const CrewRosterSkeleton = () => (
  <div className={GRID_CLASS} aria-hidden='true'>
    {[0, 1, 2, 3].map((slot) => (
      <Skeleton key={slot} className='h-12' />
    ))}
  </div>
);

export default CrewRosterPicker;
