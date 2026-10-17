import { MoreHorizontal } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import type { NotificationLevel } from "@/types/notifications";
import { FLOORS, type PrefRow } from "../utils";

const INHERIT = "inherit";

interface FloorMenuProps {
  row: PrefRow;
  /** The row's accessible name. */
  name: string;
  testId: string;
  /** `null` goes back to inheriting (on the source: no floor at all). */
  onFloor: (level: NotificationLevel | null) => void;
}

/**
 * A row's level floor, behind "…": below it, kinds go to the inbox only. A
 * category or kind can inherit ("Same as …") or set its own, including
 * "Everything" to let through what a row above holds back. On the source,
 * "Everything" is simply no floor.
 */
export function FloorMenu({ row, name, testId, onFloor }: FloorMenuProps) {
  const isSource = row.parent === null;
  const value = row.floor ?? (isSource ? "passive" : INHERIT);
  const pick = (next: string) => {
    if (next === value) return;
    if (next === INHERIT || (isSource && next === "passive")) onFloor(null);
    else onFloor(next as NotificationLevel);
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='size-7'
          aria-label={`${name}: level`}
          data-testid={`${testId}-menu`}
        >
          <MoreHorizontal />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' data-testid={`${testId}-floor`}>
        <DropdownMenuLabel className='text-muted-foreground text-xs'>Send</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={value} onValueChange={pick}>
          {!isSource && (
            <DropdownMenuRadioItem value={INHERIT} data-testid={`${testId}-floor-inherit`}>
              Same as {row.parent}
            </DropdownMenuRadioItem>
          )}
          {FLOORS.map((floor) => (
            <DropdownMenuRadioItem
              key={floor.value}
              value={floor.value}
              data-testid={`${testId}-floor-${floor.value}`}
            >
              {floor.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
